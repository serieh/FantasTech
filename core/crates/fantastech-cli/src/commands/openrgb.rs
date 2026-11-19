//! Standalone OpenRGB TCP server command implementation.

use fantastech_device::Device;
use fantastech_protocol::constants::SPARSE_KEY_COUNT;
use fantastech_protocol::lighting::RgbColor;
use fantastech_protocol::openrgb::normalize_brightness_to_percent;
use fantastech_transport::Transport;
use crate::service::{DeviceControl, DeviceHolder, handle_openrgb_client};
use std::net::TcpListener;
use std::sync::atomic::AtomicU8;
use std::sync::{Arc, Mutex};

/// Runs the OpenRGB SDK Server daemon on a TCP port (default: 6742).
pub(crate) fn run_openrgb_server<T: Transport + 'static>(
    mut dev: Device<T, fantastech_device::ReversibleAccess>,
    bind: &str,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("{}:{}", bind, port);
    let listener = TcpListener::bind(&addr)?;

    println!("🌐 ================================================================");
    println!("🌐 Fantech MK894 OpenRGB SDK Server Daemon");
    println!("🌐 ================================================================");
    println!("   Listening Address:   tcp://{}", addr);
    println!("   Protocol Version:    OpenRGB SDK v3");
    println!("   Device Integration:  Fantech MK894 Maxpower II (104 Mechanical Keys)");
    println!();
    println!("💡 Connect OpenRGB (desktop GUI or CLI) using Client Mode -> 127.0.0.1:{}", port);
    println!("⌨️  Press Ctrl+C to terminate the daemon.");
    println!("🌐 ================================================================");
    println!();

    // Query active hardware lighting preset to reflect true hardware state in OpenRGB GUI
    let hw_cfg = dev.query_light_config().ok();
    let (active_mode, active_brightness, initial_color) = if let Some(ref cfg) = hw_cfg {
        println!(
            "💡 Keyboard hardware state detected: Mode {:?}, Brightness {}, Color RGB({}, {}, {})",
            cfg.mode, cfg.brightness, cfg.color.r, cfg.color.g, cfg.color.b
        );
        let color = if cfg.color != RgbColor::new(0, 0, 0) {
            cfg.color
        } else {
            RgbColor::new(255, 0, 0)
        };
        (
            cfg.mode as u8,
            normalize_brightness_to_percent(cfg.brightness as u32),
            color,
        )
    } else {
        (16u8, 100u8, RgbColor::new(255, 0, 0))
    };

    let cached_matrix = Arc::new(Mutex::new([initial_color; SPARSE_KEY_COUNT]));
    let openrgb_brightness = Arc::new(AtomicU8::new(active_brightness));
    let openrgb_mode = Arc::new(AtomicU8::new(active_mode));
    let dev_control: Arc<Mutex<Box<dyn DeviceControl>>> =
        Arc::new(Mutex::new(Box::new(DeviceHolder::Reversible(dev))));

    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("⚠️ Incoming TCP connection error: {e}");
                continue;
            }
        };

        let peer = stream.peer_addr().map(|p| p.to_string()).unwrap_or_else(|_| "Unknown".into());
        println!("🤝 OpenRGB client connected from {}", peer);
        let _ = stream.set_nodelay(true);

        let dev_ref = Arc::clone(&dev_control);
        let mat_ref = Arc::clone(&cached_matrix);
        let brt_ref = Arc::clone(&openrgb_brightness);
        let mode_ref = Arc::clone(&openrgb_mode);

        std::thread::spawn(move || {
            handle_openrgb_client(stream, dev_ref, mat_ref, brt_ref, mode_ref);
            println!("👋 Client {} disconnected.", peer);
        });
    }

    Ok(())
}
