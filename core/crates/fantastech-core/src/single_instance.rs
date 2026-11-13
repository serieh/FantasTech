//! FantasTech — Single Instance & IPC Focus Manager
//!
//! Enforces strict single-instance application lifecycle:
//! - Only ONE GUI window is ever allowed to run at any time.
//! - Subsequent launches or tray clicks focus/restore the existing window.
//! - Only ONE System Tray (StatusNotifierItem) is allowed to run.
//! - Stale Unix domain sockets from crashes/unclean exits are automatically reclaimed.
//!
//! License: Apache-2.0

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

fn is_dir_writable(path: &Path) -> bool {
    if !path.exists() && std::fs::create_dir_all(path).is_err() {
        return false;
    }
    let test_file = path.join(format!(".write_test_{}", std::process::id()));
    if std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&test_file)
        .is_ok()
    {
        let _ = std::fs::remove_file(&test_file);
        true
    } else {
        false
    }
}

fn resolve_safe_socket_path(filename: &str) -> PathBuf {
    if let Ok(custom) = std::env::var("FANTASTECH_SOCKET_DIR") {
        let p = PathBuf::from(custom.trim());
        if p.exists() {
            return p.join(filename);
        }
    }

    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let p = PathBuf::from(runtime_dir.trim());
        if is_dir_writable(&p) {
            return p.join(filename);
        }
    }

    let cfg_dir = crate::config::get_fantastech_config_dir();
    if is_dir_writable(&cfg_dir) {
        return cfg_dir.join(filename);
    }

    std::env::temp_dir().join(filename)
}

/// Standard Unix domain socket path for the single-instance GUI window.
pub fn single_instance_socket_path() -> PathBuf {
    resolve_safe_socket_path("fantastech_gui.sock")
}

/// Standard Unix domain socket path for the single-instance System Tray.
pub fn tray_socket_path() -> PathBuf {
    resolve_safe_socket_path("fantastech_tray.sock")
}

/// Attempts to connect to an existing running FantasTech GUI instance and bring its window to focus.
///
/// Returns `true` if an existing instance was found and signaled, or `false` if no instance is active.
pub fn focus_existing_gui() -> bool {
    focus_existing_gui_with_action("FOCUS")
}

/// Attempts to connect to an existing running FantasTech GUI instance, bring it to focus,
/// and pass an action command (such as "FOCUS" or "UPDATE").
#[cfg(unix)]
pub fn focus_existing_gui_with_action(action: &str) -> bool {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    let socket_path = single_instance_socket_path();
    match UnixStream::connect(&socket_path) {
        Ok(mut stream) => {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
            let payload = format!("{}\n", action.trim());
            if stream.write_all(payload.as_bytes()).is_ok() && stream.flush().is_ok() {
                let mut reader = BufReader::new(&mut stream);
                let mut resp = String::new();
                let _ = reader.read_line(&mut resp);
                return true;
            }
            true
        }
        Err(_) => {
            // Not running or stale socket
            let _ = std::fs::remove_file(&socket_path);
            false
        }
    }
}

#[cfg(target_os = "windows")]
pub fn focus_existing_gui_with_action(action: &str) -> bool {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    match TcpStream::connect(("127.0.0.1", crate::ipc::WINDOWS_GUI_PORT)) {
        Ok(mut stream) => {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
            let payload = format!("{}\n", action.trim());
            if stream.write_all(payload.as_bytes()).is_ok() && stream.flush().is_ok() {
                let mut reader = BufReader::new(&mut stream);
                let mut resp = String::new();
                let _ = reader.read_line(&mut resp);
                return true;
            }
            true
        }
        Err(_) => false,
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
pub fn focus_existing_gui_with_action(_action: &str) -> bool {
    false
}

/// Checks whether an existing System Tray instance is active.
#[cfg(unix)]
pub fn is_tray_already_running() -> bool {
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    let socket_path = tray_socket_path();
    match UnixStream::connect(&socket_path) {
        Ok(stream) => {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(150)));
            let mut reader = BufReader::new(stream);
            let mut resp = String::new();
            let _ = reader.read_line(&mut resp);
            true
        }
        Err(_) => {
            let _ = std::fs::remove_file(&socket_path);
            false
        }
    }
}

#[cfg(target_os = "windows")]
pub fn is_tray_already_running() -> bool {
    use std::io::{BufRead, BufReader};
    use std::net::TcpStream;
    use std::time::Duration;
    if let Ok(stream) = TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], crate::ipc::WINDOWS_TRAY_PORT)),
        Duration::from_millis(250),
    ) {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
        let mut reader = BufReader::new(stream);
        let mut resp = String::new();
        let _ = reader.read_line(&mut resp);
        true
    } else {
        false
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
pub fn is_tray_already_running() -> bool {
    false
}

/// RAII Guard that owns the single-instance GUI socket and listener thread.
pub struct SingleInstanceGuard {
    #[cfg(unix)]
    socket_path: PathBuf,
    #[cfg(any(unix, target_os = "windows"))]
    shutdown_tx: Option<std::sync::mpsc::Sender<()>>,
}

impl SingleInstanceGuard {
    /// Binds the single-instance socket and spawns the listener thread.
    /// Whenever a focus/action request is received, `focus_tx` is signaled and `on_wake` is called.
    #[cfg(unix)]
    pub fn bind<F>(
        focus_tx: std::sync::mpsc::Sender<String>,
        on_wake: F,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        F: Fn() + Send + Sync + 'static,
    {
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;
        use std::sync::Arc;
        use std::time::Duration;

        let socket_path = single_instance_socket_path();
        if let Some(parent) = socket_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;

        let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();
        let path_clone = socket_path.clone();
        let wake_cb = Arc::new(on_wake);

        std::thread::spawn(move || {
            loop {
                if shutdown_rx.try_recv().is_ok() {
                    break;
                }

                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut reader = BufReader::new(&mut stream);
                        let mut line = String::new();
                        if reader.read_line(&mut line).is_ok() {
                            let action = line.trim();
                            if !action.is_empty() {
                                let _ = focus_tx.send(action.to_string());
                                wake_cb();
                                let _ = stream.write_all(b"OK\n");
                                let _ = stream.flush();
                            }
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
            let _ = std::fs::remove_file(&path_clone);
        });

        Ok(Self {
            socket_path,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    #[cfg(target_os = "windows")]
    pub fn bind<F>(
        focus_tx: std::sync::mpsc::Sender<String>,
        on_wake: F,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        F: Fn() + Send + Sync + 'static,
    {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::sync::Arc;
        use std::time::Duration;

        let listener = TcpListener::bind(("127.0.0.1", crate::ipc::WINDOWS_GUI_PORT))?;
        listener.set_nonblocking(true)?;

        let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();
        let wake_cb = Arc::new(on_wake);

        std::thread::spawn(move || {
            loop {
                if shutdown_rx.try_recv().is_ok() {
                    break;
                }

                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut reader = BufReader::new(&mut stream);
                        let mut line = String::new();
                        if reader.read_line(&mut line).is_ok() {
                            let action = line.trim();
                            if !action.is_empty() {
                                let _ = focus_tx.send(action.to_string());
                                wake_cb();
                                let _ = stream.write_all(b"OK\n");
                                let _ = stream.flush();
                            }
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            shutdown_tx: Some(shutdown_tx),
        })
    }

    #[cfg(not(any(unix, target_os = "windows")))]
    pub fn bind<F>(
        _focus_tx: std::sync::mpsc::Sender<String>,
        _on_wake: F,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        F: Fn() + Send + Sync + 'static,
    {
        Ok(Self {})
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        #[cfg(any(unix, target_os = "windows"))]
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

/// RAII Guard that owns the single-instance System Tray socket or port.
pub struct TrayInstanceGuard {
    #[cfg(unix)]
    socket_path: PathBuf,
    #[cfg(any(unix, target_os = "windows"))]
    shutdown_tx: Option<std::sync::mpsc::Sender<()>>,
}

impl TrayInstanceGuard {
    #[cfg(unix)]
    pub fn bind() -> Option<Self> {
        use std::io::Write;
        use std::os::unix::net::UnixListener;
        let socket_path = tray_socket_path();
        if let Some(parent) = socket_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::remove_file(&socket_path);
        let listener = UnixListener::bind(&socket_path).ok()?;
        let _ = listener.set_nonblocking(true);

        let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();
        let path_clone = socket_path.clone();

        std::thread::spawn(move || {
            loop {
                if shutdown_rx.try_recv().is_ok() {
                    break;
                }
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.write_all(b"TRAY_OK\n");
                        let _ = stream.flush();
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            }
            let _ = std::fs::remove_file(&path_clone);
        });

        Some(Self {
            socket_path,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    #[cfg(target_os = "windows")]
    pub fn bind() -> Option<Self> {
        use std::io::Write;
        use std::net::TcpListener;
        let listener = TcpListener::bind(("127.0.0.1", crate::ipc::WINDOWS_TRAY_PORT)).ok()?;
        let _ = listener.set_nonblocking(true);

        let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            loop {
                if shutdown_rx.try_recv().is_ok() {
                    break;
                }
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.write_all(b"TRAY_OK\n");
                        let _ = stream.flush();
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            }
        });

        Some(Self {
            shutdown_tx: Some(shutdown_tx),
        })
    }

    #[cfg(not(any(unix, target_os = "windows")))]
    pub fn bind() -> Option<Self> {
        Some(Self {})
    }
}

impl Drop for TrayInstanceGuard {
    fn drop(&mut self) {
        #[cfg(any(unix, target_os = "windows"))]
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_instance_paths_non_empty() {
        let gui_sock = single_instance_socket_path();
        assert!(gui_sock.to_str().unwrap().contains("fantastech_gui.sock"));

        let tray_sock = tray_socket_path();
        assert!(tray_sock.to_str().unwrap().contains("fantastech_tray.sock"));
    }

    #[cfg(unix)]
    #[test]
    fn test_single_instance_guard_lifecycle() {
        let (tx, rx) = std::sync::mpsc::channel();
        let guard = SingleInstanceGuard::bind(tx, || {}).expect("Failed to bind guard");

        assert!(single_instance_socket_path().exists());

        // Second check should detect it's running
        assert!(focus_existing_gui());

        // The receiver should have received a focus signal
        assert_eq!(rx.recv_timeout(std::time::Duration::from_millis(500)).unwrap(), "FOCUS");

        drop(guard);
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert!(!focus_existing_gui());
    }

    #[cfg(unix)]
    #[test]
    fn test_tray_guard_lifecycle() {
        let guard = TrayInstanceGuard::bind().expect("Failed to bind tray guard");
        assert!(is_tray_already_running());

        drop(guard);
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert!(!is_tray_already_running());
    }
}
