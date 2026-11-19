//! fantastech-cli regression tests.

use super::*;
use std::time::{Duration, Instant};
#[cfg(target_os = "linux")]
use crate::volume_guard::{DebounceAction, RawInputEvent, VolumeDebouncer};
#[cfg(target_os = "linux")]
const EV_SYN: u16 = 0x00;
#[cfg(target_os = "linux")]
const EV_KEY: u16 = 0x01;
#[cfg(target_os = "linux")]
const SYN_REPORT: u16 = 0;
#[cfg(target_os = "linux")]
const KEY_VOLUMEUP: u16 = 115;

#[cfg(target_os = "linux")]
#[test]
fn test_raw_input_event_roundtrip() {
    let ev = RawInputEvent::new(EV_KEY, KEY_VOLUMEUP, 1);
    let bytes = ev.to_bytes();
    let parsed = RawInputEvent::from_bytes(&bytes);
    assert_eq!(ev, parsed);
}

#[cfg(target_os = "linux")]
#[test]
fn test_volume_debouncer_logic() {
    let mut debouncer = VolumeDebouncer::default();
    let t0 = Instant::now();

    // 1. Initial Volume Up press -> Emit pulse
    let ev_press = RawInputEvent::new(EV_KEY, KEY_VOLUMEUP, 1);
    let action = debouncer.process_event(&ev_press, t0);
    assert_eq!(
        action,
        DebounceAction::EmitPulseWithAutoRelease {
            code: KEY_VOLUMEUP,
            release_delay: Duration::from_millis(5),
        }
    );

    // 2. Chattering second press 10ms later (<25ms) -> Dropped
    let t1 = t0 + Duration::from_millis(10);
    let action2 = debouncer.process_event(&ev_press, t1);
    assert_eq!(action2, DebounceAction::Drop);

    // 3. Hardware release event (val = 0) -> Dropped
    let ev_release = RawInputEvent::new(EV_KEY, KEY_VOLUMEUP, 0);
    let action_rel = debouncer.process_event(&ev_release, t1);
    assert_eq!(action_rel, DebounceAction::Drop);

    // 4. Hardware repeat event (val = 2) -> Dropped
    let ev_rep = RawInputEvent::new(EV_KEY, KEY_VOLUMEUP, 2);
    let action_rep = debouncer.process_event(&ev_rep, t1);
    assert_eq!(action_rep, DebounceAction::Drop);

    // 5. Subsequent valid press 35ms after t0 (>=25ms) -> Emitted
    let t2 = t0 + Duration::from_millis(35);
    let action3 = debouncer.process_event(&ev_press, t2);
    assert_eq!(
        action3,
        DebounceAction::EmitPulseWithAutoRelease {
            code: KEY_VOLUMEUP,
            release_delay: Duration::from_millis(5),
        }
    );

    // 6. Normal media key (KEY_MUTE = 113) -> Forward as-is
    let ev_mute = RawInputEvent::new(EV_KEY, 113, 1);
    let action_mute = debouncer.process_event(&ev_mute, t2);
    assert_eq!(action_mute, DebounceAction::ForwardAsIs);

    // 7. EV_SYN event -> Forward as-is
    let ev_syn = RawInputEvent::new(EV_SYN, SYN_REPORT, 0);
    let action_syn = debouncer.process_event(&ev_syn, t2);
    assert_eq!(action_syn, DebounceAction::ForwardAsIs);
}

#[test]
fn test_single_executable_cli_and_gui_dispatch() {
    // When run with no arguments, subcommand is None (defaults to GUI launch)
    let cli_empty = Cli::try_parse_from(["fantastech"]).unwrap();
    assert!(cli_empty.command.is_none());
    assert!(!cli_empty.gui);

    // When run with --gui flag
    let cli_gui_flag = Cli::try_parse_from(["fantastech", "--gui"]).unwrap();
    assert!(cli_gui_flag.gui);

    // When run with gui subcommand
    let cli_gui_cmd = Cli::try_parse_from(["fantastech", "gui"]).unwrap();
    assert!(matches!(cli_gui_cmd.command, Some(Commands::Gui(_))));

    // When run with info subcommand
    let cli_info = Cli::try_parse_from(["fantastech", "info"]).unwrap();
    assert!(matches!(cli_info.command, Some(Commands::Info)));
}

#[cfg(unix)]
#[test]
fn test_single_instance_gui_and_tray_dispatch() {
    let (tx, rx) = std::sync::mpsc::channel();
    let guard = fantastech_core::SingleInstanceGuard::bind(tx, || {}).expect("Failed to bind guard");

    // When GUI is active, focus_existing_gui() must return true
    assert!(fantastech_core::focus_existing_gui());
    assert_eq!(rx.recv_timeout(std::time::Duration::from_millis(500)).unwrap(), "FOCUS");

    // Single instance can also dispatch EXIT to request GUI closure
    assert!(fantastech_core::focus_existing_gui_with_action("EXIT"));
    assert_eq!(rx.recv_timeout(std::time::Duration::from_millis(500)).unwrap(), "EXIT");

    // When tray is active, is_tray_already_running() must return true
    let tray_guard = fantastech_core::TrayInstanceGuard::bind().expect("Failed to bind tray guard");
    assert!(fantastech_core::is_tray_already_running());

    // Calling run_tray_loop while tray is already active returns Ok(()) immediately
    assert!(tray::run_tray_loop().is_ok());

    drop(tray_guard);
    drop(guard);
}
