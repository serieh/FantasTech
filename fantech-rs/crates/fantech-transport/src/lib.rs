//! Transport abstractions and hardware communication backends for Fantech MK894.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

use fantech_protocol::constants::{PRODUCT_ID_AK110_DS, PRODUCT_ID_MK894, REPORT_LENGTH};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

use rustix::event::{poll, PollFd, PollFlags};
use std::os::unix::io::AsFd;

/// Transport-level errors.
#[derive(Error, Debug)]
pub enum TransportError {
    /// Device was not found on the host system.
    #[error("No compatible Fantech MK894 device found in /dev/hidraw nodes")]
    DeviceNotFound,

    /// Permission denied opening device node.
    #[error("Permission denied opening {0}. Ensure udev rule with TAG+=\"uaccess\" is installed or run with proper user permissions.")]
    PermissionDenied(PathBuf),

    /// Standard I/O error during communication.
    #[error("I/O error communicating with USB HID device: {0}")]
    Io(#[from] std::io::Error),

    /// Read or write timed out.
    #[error("Communication timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// Invalid buffer length during transfer.
    #[error("Invalid transfer buffer length: expected 64 bytes, got {0}")]
    InvalidLength(usize),
}

/// Abstract transport trait for sending and receiving 64-byte USB Output/Feature Reports.
pub trait Transport: Send + Sync {
    /// Reads a 64-byte report from the device into the provided buffer.
    fn read_report(&mut self, buf: &mut [u8; REPORT_LENGTH]) -> Result<usize, TransportError>;

    /// Writes a 64-byte report to the device.
    fn write_report(&mut self, buf: &[u8; REPORT_LENGTH]) -> Result<(), TransportError>;
}

/// In-memory mock transport for unit testing and emulator integration without physical hardware.
#[derive(Default)]
pub struct MockTransport {
    /// Queue of reports to be returned by `read_report`.
    pub incoming: VecDeque<[u8; REPORT_LENGTH]>,
    /// History of reports sent via `write_report`.
    pub outgoing: Vec<[u8; REPORT_LENGTH]>,
}

impl MockTransport {
    /// Creates a new empty mock transport.
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues an incoming report to be read by the client.
    pub fn push_incoming(&mut self, report: [u8; REPORT_LENGTH]) {
        self.incoming.push_back(report);
    }
}

impl Transport for MockTransport {
    fn read_report(&mut self, buf: &mut [u8; REPORT_LENGTH]) -> Result<usize, TransportError> {
        if let Some(data) = self.incoming.pop_front() {
            buf.copy_from_slice(&data);
            Ok(REPORT_LENGTH)
        } else {
            Err(TransportError::Io(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "No mock report available",
            )))
        }
    }

    fn write_report(&mut self, buf: &[u8; REPORT_LENGTH]) -> Result<(), TransportError> {
        self.outgoing.push(*buf);
        Ok(())
    }
}

/// Linux `hidraw` hardware transport.
pub struct HidrawTransport {
    file: File,
    path: PathBuf,
}

impl HidrawTransport {
    /// Opens an explicit `/dev/hidrawX` path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, TransportError> {
        let p = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&p)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    TransportError::PermissionDenied(p.clone())
                } else {
                    TransportError::Io(e)
                }
            })?;
        Ok(Self { file, path: p })
    }

    /// Automatically scans `/sys/class/hidraw` to locate Interface 2 of the MK894.
    pub fn discover() -> Result<Self, TransportError> {
        let sys_path = Path::new("/sys/class/hidraw");
        if !sys_path.exists() {
            return Err(TransportError::DeviceNotFound);
        }

        let entries = std::fs::read_dir(sys_path).map_err(TransportError::Io)?;
        for entry in entries.flatten() {
            let uevent_path = entry.path().join("device/uevent");
            if let Ok(content) = std::fs::read_to_string(&uevent_path) {
                // Match SDCX Vendor ID 0x36AE and supported Product IDs
                let is_vid = content.contains("v000036AE") || content.contains("36ae") || content.contains("36AE");
                let is_pid = content.contains(&format!("{:04X}", PRODUCT_ID_MK894))
                    || content.contains(&format!("{:04X}", PRODUCT_ID_AK110_DS));
                
                // Interface 2 corresponds to input2 / .0002
                let is_interface_2 = content.contains("input2") || content.contains(".0002");

                if is_vid && is_pid && is_interface_2 {
                    let dev_node = Path::new("/dev").join(entry.file_name());
                    tracing::info!("Discovered Fantech MK894 configuration node at {:?}", dev_node);
                    return Self::open(dev_node);
                }
            }
        }

        Err(TransportError::DeviceNotFound)
    }

    /// Discards any stale unread reports currently queued in the kernel hidraw buffer.
    pub fn drain(&mut self) {
        let mut discard = [0u8; REPORT_LENGTH];
        loop {
            let mut poll_fds = [PollFd::from_borrowed_fd(self.file.as_fd(), PollFlags::IN)];
            match poll(&mut poll_fds, 0) {
                Ok(n) if n > 0 && poll_fds[0].revents().contains(PollFlags::IN) => {
                    if self.file.read(&mut discard).is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
    }

    /// Returns the active device node path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Transport for HidrawTransport {
    fn read_report(&mut self, buf: &mut [u8; REPORT_LENGTH]) -> Result<usize, TransportError> {
        let mut poll_fds = [PollFd::from_borrowed_fd(self.file.as_fd(), PollFlags::IN)];
        let poll_res = poll(&mut poll_fds, 2000).map_err(std::io::Error::from)?;
        if poll_res == 0 || !poll_fds[0].revents().contains(PollFlags::IN) {
            return Err(TransportError::Timeout(std::time::Duration::from_millis(2000)));
        }

        let n = self.file.read(buf).map_err(TransportError::Io)?;
        tracing::debug!("📥 [HID RX] ({} bytes): {:02X?}", n, &buf[..n.min(16)]);
        Ok(n)
    }

    fn write_report(&mut self, buf: &[u8; REPORT_LENGTH]) -> Result<(), TransportError> {
        tracing::debug!("📤 [HID TX] ({} bytes): {:02X?}", buf.len(), &buf[..16]);
        self.drain();
        self.file.write_all(buf).map_err(TransportError::Io)?;
        self.file.flush().map_err(TransportError::Io)?;

        self.drain();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_transport_roundtrip() {
        let mut mock = MockTransport::new();
        let test_packet = [0x42u8; REPORT_LENGTH];
        mock.write_report(&test_packet).unwrap();
        assert_eq!(mock.outgoing.len(), 1);
        assert_eq!(mock.outgoing[0], test_packet);

        let incoming_packet = [0x99u8; REPORT_LENGTH];
        mock.push_incoming(incoming_packet);
        let mut read_buf = [0u8; REPORT_LENGTH];
        let n = mock.read_report(&mut read_buf).unwrap();
        assert_eq!(n, REPORT_LENGTH);
        assert_eq!(read_buf, incoming_packet);
    }
}
