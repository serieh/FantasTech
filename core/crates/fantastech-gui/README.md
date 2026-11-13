# FantasTech GUI refactor

This is a source-level modernization of the supplied FantasTech GUI for the Fantech MK894 Maxpower II. It is intended to replace the GUI crate's `Cargo.toml` and `src/` contents inside the original workspace; it is **not** a standalone workspace because it still consumes the existing local backend crates.

## Structure

- `src/main.rs` — application entry point, shared types, keyboard layout data, and application state.
- `src/ui.rs` — egui rendering, navigation, dialogs, page layouts, and GUI regression tests.
- `src/commands.rs` — profile/config persistence and device operations. Existing IPC request types, device APIs, protocol types, and persistent-write authorization remain in use.
- `src/platform.rs` — background connection monitoring and OS integration adapters.

## Implemented in this pass

- Split the original 4,190-line source file into UI, commands, and platform modules.
- Moved service-state checks and HID discovery into a background connection monitor. It checks every two seconds and supports explicit refresh requests; the UI frame callback only consumes snapshots.
- Removed Linux-specific system font file reads; the GUI now uses eframe's bundled default font set, consistent spacing defaults, and cleaner text-first controls instead of emoji-heavy button labels.
- Set the default window to 1520×960 logical points and the minimum to 1240×720; compact top navigation replaces the desktop side rail below 1400 points.
- Made the primary keyboard canvases scale to the available width with larger keycaps and legible labels; the mini action-palette keyboard remains compact.
- Reworked key-mapping, lighting, macro, header, permission and settings toolbars to wrap instead of allowing controls to collide or overlap. Lighting cards wrap as a group when required.
- Replaced the previous color control with a circular HSV picker: hue is selected by angle, saturation by distance from center, and brightness by a separate slider. A smooth triangle mesh renders the disc in one paint shape; RGB numeric controls, format display, and quick swatches remain available.
- Refined the dark/light palettes, control rounding, spacing and panel contrast without changing profile or hardware command formats.
- Added a searchable quick-action palette and bounded (100-entry) undo/redo for active keymap edits, including mirrored-key changes and imported-profile history resets.
- Centralized per-key RGB operations in the command layer and corrected dry-run behavior so the IPC per-key color path does not bypass safe mode.
- Scoped eframe's X11/Wayland feature selection to Linux and isolated Linux-only systemd/udev installation behavior behind platform adapters. Permission guidance now avoids showing Linux-specific commands on other operating systems, and connection-monitor thread startup failures are surfaced in the status area.

## Preserved boundaries

The GUI continues to use the existing `fantastech-protocol`, `fantastech-transport`, `fantastech-device`, `fantastech-core`, and `fantastech-emulator` crates. No protocol report formats, IPC request/response definitions, profile JSON schemas, macro-memory layouts, or persistent-write authorization strings were intentionally changed.

## Integration

Copy this crate's `Cargo.toml` and `src/` into the original workspace's `fantastech-gui/` directory. The paths in the manifest expect the five existing sibling backend crates and the original workspace root manifest.

From the original workspace root, run:

```sh
cargo fmt --all -- --check
cargo check --workspace
cargo test -p fantastech-gui
```

Then validate the GUI in emulator mode before attaching the physical keyboard. Check service-based operation, direct HID operation, dry-run, profile import/export, backup/restore, macro recording/flashing, and factory-reset confirmation separately.

## Verification limitations of this handoff

Only the GUI `Cargo.toml` and `main.rs` were present in the supplied files. The workspace root manifest and the five local backend crates were not available, and this execution environment did not have `rustc` or `cargo`. Therefore this pass could not run Rust formatting, compilation, unit tests, hardware tests, or cross-target builds. Windows/macOS GUI compilation may be possible with the target-specific eframe features, but keyboard transport and daemon support still depend on the unavailable backend crates and require separate verification.

Direct user-triggered HID/IPC mutations still use the existing synchronous backend calls in this pass. The recurring connection scan no longer blocks egui, but a slow device write can still delay its initiating UI action. A fully asynchronous serialized device-command queue requires inspection and testing of the missing transport/device/service implementations before it can safely be introduced.
