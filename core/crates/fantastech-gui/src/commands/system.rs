//! Platform desktop and systemd integration/uninstallation.

use super::*;

impl FantasTechApp {
    #[cfg(target_os = "linux")]
    #[allow(dead_code)]
    pub(crate) fn install_system_integration(&mut self) {
        let mut logs = Vec::new();
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let bin_dir = std::path::PathBuf::from(&home).join(".local/bin");
        let desktop_dir = std::path::PathBuf::from(&home).join(".local/share/applications");
        let icon_dir = std::path::PathBuf::from(&home).join(".local/share/icons/hicolor/scalable/apps");
        let systemd_dir = std::path::PathBuf::from(&home).join(".config/systemd/user");

        let _ = std::fs::create_dir_all(&bin_dir);
        let _ = std::fs::create_dir_all(&desktop_dir);
        let _ = std::fs::create_dir_all(&icon_dir);
        let _ = std::fs::create_dir_all(&systemd_dir);

        if let Ok(cur_exe) = std::env::current_exe() {
            if let Some(parent) = cur_exe.parent() {
                let cli_src = parent.join("fantastech");
                let gui_src = parent.join("fantastech-gui");
                if cli_src.exists() {
                    let _ = std::fs::copy(&cli_src, bin_dir.join("fantastech"));
                    let _ = std::fs::copy(&cli_src, bin_dir.join("fantastech-ctl"));
                    logs.push("Installed fantastech and fantastech-ctl to ~/.local/bin/");
                }
                if gui_src.exists() {
                    let _ = std::fs::copy(&gui_src, bin_dir.join("fantastech-gui"));
                    logs.push("Installed fantastech-gui to ~/.local/bin/");
                }
            }
        }

        let desktop_content = "[Desktop Entry]\nName=FantasTech\nComment=Fantech MK894 Mechanical RGB Keyboard Control Center\nExec=fantastech-gui\nIcon=fantastech\nTerminal=false\nType=Application\nCategories=Utility;HardwareSettings;\nKeywords=rgb;keyboard;lighting;macro;openrgb;fantastech;\nStartupNotify=true\n";
        let _ = write_user_owned_file(&desktop_dir.join("fantastech.desktop"), desktop_content.as_bytes());
        logs.push("Installed desktop launcher to ~/.local/share/applications/fantastech.desktop");

        let svg_content = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
  <rect width="64" height="64" rx="14" fill="#E53E3E"/>
  <rect x="8" y="14" width="48" height="36" rx="6" fill="#1E293B" stroke="#FFFFFF" stroke-opacity="0.7" stroke-width="1.5"/>
  <circle cx="16" cy="24" r="3" fill="#0071CF"/>
  <circle cx="24" cy="24" r="3" fill="#F59E0B"/>
  <circle cx="32" cy="24" r="3" fill="#10B981"/>
  <circle cx="40" cy="24" r="3" fill="#06B6D4"/>
  <circle cx="48" cy="24" r="3" fill="#A855F7"/>
  <circle cx="16" cy="34" r="3" fill="#EC4899"/>
  <circle cx="24" cy="34" r="3" fill="#FFFFFF"/>
  <circle cx="32" cy="34" r="3" fill="#0071CF"/>
  <circle cx="40" cy="34" r="3" fill="#F59E0B"/>
  <circle cx="48" cy="34" r="3" fill="#10B981"/>
  <rect x="20" y="42" width="24" height="4" rx="2" fill="#FFFFFF"/>
</svg>"##;
        let _ = write_user_owned_file(&icon_dir.join("fantastech.svg"), svg_content.as_bytes());
        logs.push("Installed scalable app icon to ~/.local/share/icons/hicolor/scalable/apps/fantastech.svg");

        let service_content = "[Unit]\nDescription=FantasTech Background Keyboard Daemon (Fantech MK894)\nDocumentation=https://github.com/serieh/fantastech\nAfter=default.target\n\n[Service]\nType=simple\nExecStart=%h/.local/bin/fantastech service run\nRestart=on-failure\nRestartSec=3\n\n[Install]\nWantedBy=default.target\n";
        let _ = write_user_owned_file(&systemd_dir.join("fantastech.service"), service_content.as_bytes());
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "enable", "--now", "fantastech.service"])
            .status();
        logs.push("Enabled and started systemd user service fantastech.service");

        self.install_status_message = Some(format!("Installation complete! ({})", logs.join("; ")));
        self.status_message = "Installed FantasTech system integration".to_string();
    }
    #[cfg(not(target_os = "linux"))]
    #[allow(dead_code)]
    pub(crate) fn install_system_integration(&mut self) {
        self.install_status_message = Some(
            "Native installer integration is currently implemented for Linux only. The GUI remains available on this platform.".to_owned(),
        );
        self.status_message = "System installation is not available on this platform".to_owned();
    }


    #[cfg(target_os = "linux")]
    #[allow(dead_code)]
    pub(crate) fn uninstall_system_integration(&mut self) {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "disable", "--now", "fantastech.service"])
            .status();
        let _ = std::fs::remove_file(std::path::PathBuf::from(&home).join(".config/systemd/user/fantastech.service"));
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
        let _ = std::fs::remove_file(std::path::PathBuf::from(&home).join(".local/share/applications/fantastech.desktop"));
        let _ = std::fs::remove_file(std::path::PathBuf::from(&home).join(".local/share/icons/hicolor/scalable/apps/fantastech.svg"));
        let _ = std::fs::remove_file(std::path::PathBuf::from(&home).join(".local/bin/fantastech"));
        let _ = std::fs::remove_file(std::path::PathBuf::from(&home).join(".local/bin/fantastech-ctl"));
        let _ = std::fs::remove_file(std::path::PathBuf::from(&home).join(".local/bin/fantastech-gui"));
        self.install_status_message = Some("Successfully uninstalled FantasTech system integration (binaries, desktop launcher, systemd service).".to_string());
        self.status_message = "Uninstalled FantasTech system integration".to_string();
    }
    #[cfg(not(target_os = "linux"))]
    #[allow(dead_code)]
    pub(crate) fn uninstall_system_integration(&mut self) {
        self.install_status_message = Some(
            "No Linux system integration was changed. Platform-specific installer support is not configured here.".to_owned(),
        );
        self.status_message = "System uninstallation is not available on this platform".to_owned();
    }


}
