//! Linux Hardware Volume Knob Guard (evdev & uinput debouncer).
//!
//! Captures raw events from the Fantech MK894 media keyboard interface,
//! filters out mechanical chatter and runaway repetition from the rotary volume knob,
//! and forwards clean, single-pulse volume up / down events to the OS.

#![allow(unsafe_code)]

use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const EV_KEY: u16 = 0x01;
const EV_SYN: u16 = 0x00;
const SYN_REPORT: u16 = 0;
const KEY_VOLUMEDOWN: u16 = 114;
const KEY_VOLUMEUP: u16 = 115;

const EVIOCGRAB: libc::c_ulong = 0x40044590;
const UI_DEV_CREATE: libc::c_ulong = 0x5501;
const UI_DEV_DESTROY: libc::c_ulong = 0x5502;
const UI_SET_EVBIT: libc::c_ulong = 0x40045564;
const UI_SET_KEYBIT: libc::c_ulong = 0x40045565;
const UI_DEV_SETUP: libc::c_ulong = 0x405c5503;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawInputEvent {
    pub time_sec: i64,
    pub time_usec: i64,
    pub type_: u16,
    pub code: u16,
    pub value: i32,
}

impl RawInputEvent {
    pub fn new(type_: u16, code: u16, value: i32) -> Self {
        Self {
            time_sec: 0,
            time_usec: 0,
            type_,
            code,
            value,
        }
    }

    pub fn to_bytes(self) -> [u8; 24] {
        let mut buf = [0u8; 24];
        buf[0..8].copy_from_slice(&self.time_sec.to_ne_bytes());
        buf[8..16].copy_from_slice(&self.time_usec.to_ne_bytes());
        buf[16..18].copy_from_slice(&self.type_.to_ne_bytes());
        buf[18..20].copy_from_slice(&self.code.to_ne_bytes());
        buf[20..24].copy_from_slice(&self.value.to_ne_bytes());
        buf
    }

    pub fn from_bytes(buf: &[u8; 24]) -> Self {
        let time_sec = i64::from_ne_bytes(buf[0..8].try_into().unwrap());
        let time_usec = i64::from_ne_bytes(buf[8..16].try_into().unwrap());
        let type_ = u16::from_ne_bytes(buf[16..18].try_into().unwrap());
        let code = u16::from_ne_bytes(buf[18..20].try_into().unwrap());
        let value = i32::from_ne_bytes(buf[20..24].try_into().unwrap());
        Self {
            time_sec,
            time_usec,
            type_,
            code,
            value,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DebounceAction {
    ForwardAsIs,
    EmitPulseWithAutoRelease { code: u16, release_delay: Duration },
    Drop,
}

pub struct VolumeDebouncer {
    debounce_threshold: Duration,
    auto_release_delay: Duration,
    last_tick: Option<Instant>,
    last_code: u16,
}

impl Default for VolumeDebouncer {
    fn default() -> Self {
        Self::new(Duration::from_millis(30), Duration::from_millis(5))
    }
}

impl VolumeDebouncer {
    pub fn new(debounce_threshold: Duration, auto_release_delay: Duration) -> Self {
        Self {
            debounce_threshold,
            auto_release_delay,
            last_tick: None,
            last_code: 0,
        }
    }

    pub fn process_event(&mut self, ev: &RawInputEvent, now: Instant) -> DebounceAction {
        if ev.type_ == EV_KEY && (ev.code == KEY_VOLUMEUP || ev.code == KEY_VOLUMEDOWN) {
            match ev.value {
                1 => {
                    // Check mechanical chatter debounce
                    if let Some(last) = self.last_tick {
                        if self.last_code == ev.code && now.duration_since(last) < self.debounce_threshold {
                            return DebounceAction::Drop;
                        }
                    }
                    self.last_tick = Some(now);
                    self.last_code = ev.code;
                    DebounceAction::EmitPulseWithAutoRelease {
                        code: ev.code,
                        release_delay: self.auto_release_delay,
                    }
                }
                0 => {
                    // Drop raw release because auto-release is emitted
                    DebounceAction::Drop
                }
                2 => {
                    // Drop autorepeat to prevent runaway volume looping
                    DebounceAction::Drop
                }
                _ => DebounceAction::ForwardAsIs,
            }
        } else {
            DebounceAction::ForwardAsIs
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct UinputSetup {
    id: InputId,
    name: [u8; 80],
    ff_effects_max: u32,
}

fn ioctl_grab(fd: RawFd, grab: bool) -> std::io::Result<()> {
    let val: libc::c_int = if grab { 1 } else { 0 };
    let ret = unsafe { libc::ioctl(fd, EVIOCGRAB, val) };
    if ret < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn create_uinput_device(uinput_fd: RawFd) -> std::io::Result<()> {
    unsafe {
        if libc::ioctl(uinput_fd, UI_SET_EVBIT, EV_KEY as libc::c_int) < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if libc::ioctl(uinput_fd, UI_SET_EVBIT, EV_SYN as libc::c_int) < 0 {
            return Err(std::io::Error::last_os_error());
        }

        for key in 1..=0x2ff {
            let _ = libc::ioctl(uinput_fd, UI_SET_KEYBIT, key as libc::c_int);
        }

        let mut name = [0u8; 80];
        let name_str = b"Fantech MK894 Volume Guard Virtual Device\0";
        name[..name_str.len()].copy_from_slice(name_str);

        let setup = UinputSetup {
            id: InputId {
                bustype: 0x03, // BUS_USB
                vendor: 0x36AE,
                product: 0xFE9C,
                version: 1,
            },
            name,
            ff_effects_max: 0,
        };

        if libc::ioctl(uinput_fd, UI_DEV_SETUP, &setup) < 0 {
            return Err(std::io::Error::last_os_error());
        }

        if libc::ioctl(uinput_fd, UI_DEV_CREATE) < 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

fn destroy_uinput_device(uinput_fd: RawFd) {
    unsafe {
        let _ = libc::ioctl(uinput_fd, UI_DEV_DESTROY);
    }
}

struct VolumeGuardSession {
    src_fd: RawFd,
    uinput_fd: RawFd,
}

impl Drop for VolumeGuardSession {
    fn drop(&mut self) {
        let _ = ioctl_grab(self.src_fd, false);
        destroy_uinput_device(self.uinput_fd);
    }
}

fn emit_event(file: &mut File, type_: u16, code: u16, value: i32) -> std::io::Result<()> {
    let ev = RawInputEvent::new(type_, code, value);
    file.write_all(&ev.to_bytes())?;
    let syn = RawInputEvent::new(EV_SYN, SYN_REPORT, 0);
    file.write_all(&syn.to_bytes())?;
    file.flush()
}

pub fn find_media_event_node() -> Option<PathBuf> {
    if let Ok(entries) = std::fs::read_dir("/dev/input/by-id") {
        let mut candidates = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let is_fantech = name.contains("fantech") || name.contains("36ae");
            let is_if01 = name.contains("if01") || name.contains("input1");
            let is_event = name.contains("event");
            let is_mouse = name.contains("mouse");
            if is_fantech && is_if01 && is_event && !is_mouse {
                let path = entry.path();
                if name.contains("if01-event-kbd") || name.contains("event-kbd") {
                    return Some(path);
                }
                candidates.push(path);
            }
        }
        if let Some(cand) = candidates.into_iter().next() {
            return Some(cand);
        }
    }

    if let Ok(entries) = std::fs::read_dir("/sys/class/input") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("event") {
                let uevent_path = entry.path().join("device/uevent");
                if let Ok(content) = std::fs::read_to_string(&uevent_path) {
                    let is_fantech = content.contains("36AE") || content.contains("36ae");
                    let is_if01 = content.contains("input1") || content.contains(":1.1") || content.contains("if01");
                    let is_mouse = content.contains("mouse") || name.contains("mouse");
                    if is_fantech && is_if01 && !is_mouse {
                        let dev_path = PathBuf::from("/dev/input").join(&name);
                        if dev_path.exists() {
                            return Some(dev_path);
                        }
                    }
                }
            }
        }
    }

    None
}

/// Runs the active Volume Guard loop in the background service daemon.
pub fn run_volume_guard_service(
    service_running: Arc<AtomicBool>,
    guard_active_flag: Arc<AtomicBool>,
) {
    println!("🛡️  Volume Guard service loop starting...");
    guard_active_flag.store(true, Ordering::SeqCst);

    while service_running.load(Ordering::SeqCst) && guard_active_flag.load(Ordering::SeqCst) {
        let dev_node = match find_media_event_node() {
            Some(p) => p,
            None => {
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
        };

        let src_file = match std::fs::OpenOptions::new().read(true).write(true).open(&dev_node) {
            Ok(f) => f,
            Err(_) => {
                // Cannot open device node directly (missing permissions or Flatpak restriction)
                std::thread::sleep(Duration::from_secs(3));
                continue;
            }
        };

        let src_fd = src_file.as_raw_fd();
        if ioctl_grab(src_fd, true).is_err() {
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }

        let uinput_file = match std::fs::OpenOptions::new().write(true).open("/dev/uinput") {
            Ok(f) => f,
            Err(_) => {
                let _ = ioctl_grab(src_fd, false);
                std::thread::sleep(Duration::from_secs(3));
                continue;
            }
        };

        let uinput_fd = uinput_file.as_raw_fd();
        if create_uinput_device(uinput_fd).is_err() {
            let _ = ioctl_grab(src_fd, false);
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }

        println!("🛡️  Volume Guard successfully acquired grab on {:?} with virtual /dev/uinput.", dev_node);
        let _session = VolumeGuardSession { src_fd, uinput_fd };
        let mut reader = std::io::BufReader::new(src_file);
        let mut writer = uinput_file;
        let mut debouncer = VolumeDebouncer::default();
        let mut ev_buf = [0u8; 24];

        // Make src_fd non-blocking so we can respond to service shutdown
        unsafe {
            let flags = libc::fcntl(src_fd, libc::F_GETFL, 0);
            libc::fcntl(src_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }

        while service_running.load(Ordering::SeqCst) && guard_active_flag.load(Ordering::SeqCst) {
            let mut pfd = libc::pollfd {
                fd: src_fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let ret = unsafe { libc::poll(&mut pfd, 1, 1000) };
            if ret > 0 {
                if (pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL)) != 0 {
                    // Keyboard was disconnected or device node closed
                    break;
                }
                if (pfd.revents & libc::POLLIN) != 0 {
                    match reader.read_exact(&mut ev_buf) {
                        Ok(()) => {
                            let ev = RawInputEvent::from_bytes(&ev_buf);
                            let now = Instant::now();

                            match debouncer.process_event(&ev, now) {
                                DebounceAction::EmitPulseWithAutoRelease { code, release_delay } => {
                                    let _ = emit_event(&mut writer, EV_KEY, code, 1);
                                    std::thread::sleep(release_delay);
                                    let _ = emit_event(&mut writer, EV_KEY, code, 0);
                                }
                                DebounceAction::Drop => {}
                                DebounceAction::ForwardAsIs => {
                                    let _ = writer.write_all(&ev_buf);
                                    let _ = writer.flush();
                                }
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // Spurious wakeup, continue loop
                        }
                        Err(_) => {
                            // Keyboard disconnected or device closed
                            break;
                        }
                    }
                }
            } else if ret < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() != std::io::ErrorKind::Interrupted {
                    break;
                }
            }
        }
    }

    guard_active_flag.store(false, Ordering::SeqCst);
    println!("🛡️  Volume Guard stopped.");
}

/// Standalone CLI command handler for `fantastech guard-volume`.
pub fn handle_guard_volume(args: &crate::GuardVolumeArgs) -> Result<(), Box<dyn std::error::Error>> {
    let dev_node = if let Some(ref p) = args.device {
        p.clone()
    } else {
        match find_media_event_node() {
            Some(p) => p,
            None => {
                eprintln!("❌ No Fantech MK894 media keyboard event node found in /dev/input/by-id/ or /dev/input/event*.");
                eprintln!("💡 You can specify the exact event device node with: --device /dev/input/eventX");
                return Err("Fantech media event node not found".into());
            }
        }
    };

    if args.daemon {
        tracing::info!("Initializing Fantech MK894 Hardware Volume Knob Guard (daemon mode)");
        tracing::info!("Target input node: {:?}", dev_node);
    } else {
        println!("🛡️  Initializing Fantech MK894 Hardware Volume Knob Guard");
        println!("   Target input node: {:?}", dev_node);
    }

    let src_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&dev_node)
        .inspect_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                eprintln!("❌ Permission denied opening {:?}.", dev_node);
                eprintln!("💡 Ensure you run with root permissions (sudo) or are a member of the 'input' group.");
            }
        })?;

    let src_fd = src_file.as_raw_fd();

    if !args.daemon {
        println!("🔒 Acquiring exclusive grab (EVIOCGRAB) on {:?}...", dev_node);
    }
    ioctl_grab(src_fd, true).map_err(|e| {
        eprintln!("❌ Failed to acquire exclusive grab on {:?}: {}", dev_node, e);
        e
    })?;
    if !args.daemon {
        println!("✅ Grab acquired! Host input subsystem will now only receive guarded events.");
    }

    let uinput_file = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/uinput")
        .map_err(|e| {
            let _ = ioctl_grab(src_fd, false);
            eprintln!("❌ Failed to open /dev/uinput: {}", e);
            e
        })?;
    let uinput_fd = uinput_file.as_raw_fd();

    if !args.daemon {
        println!("⚙️  Creating virtual input device on /dev/uinput...");
    }
    if let Err(e) = create_uinput_device(uinput_fd) {
        let _ = ioctl_grab(src_fd, false);
        eprintln!("❌ Failed to create virtual uinput device: {}", e);
        return Err(e.into());
    }
    if !args.daemon {
        println!("✅ Virtual device 'Fantech MK894 Volume Guard Virtual Device' active!");
    }

    let _session = VolumeGuardSession { src_fd, uinput_fd };

    if !args.daemon {
        println!("🚀 Volume Guard is running! Debounce window: 30ms • Forced auto-release: 5ms");
        println!("   Press Ctrl+C to terminate cleanly.");
    } else {
        tracing::info!("Volume Guard active: 30ms debounce, 5ms forced auto-release");
    }

    let mut reader = std::io::BufReader::new(src_file);
    let mut writer = uinput_file;
    let mut debouncer = VolumeDebouncer::default();
    let mut ev_buf = [0u8; 24];

    loop {
        if reader.read_exact(&mut ev_buf).is_err() {
            break;
        }
        let ev = RawInputEvent::from_bytes(&ev_buf);
        let now = Instant::now();

        match debouncer.process_event(&ev, now) {
            DebounceAction::EmitPulseWithAutoRelease { code, release_delay } => {
                tracing::debug!("🔊 Guard emitted discrete volume pulse for code {}", code);
                emit_event(&mut writer, EV_KEY, code, 1)?;
                std::thread::sleep(release_delay);
                emit_event(&mut writer, EV_KEY, code, 0)?;
            }
            DebounceAction::Drop => {
                tracing::debug!("🛡️ Guard dropped chattering/stuck event: code={}, val={}", ev.code, ev.value);
            }
            DebounceAction::ForwardAsIs => {
                writer.write_all(&ev_buf)?;
                let _ = writer.flush();
            }
        }
    }

    Ok(())
}
