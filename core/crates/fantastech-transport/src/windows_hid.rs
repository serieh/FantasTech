//! Windows USB HID hardware transport for Fantech MK894.
//!
//! Interfaces directly with Windows HID (`hid.dll`) and SetupAPI (`setupapi.dll`)
//! using native Win32 device discovery and overlapped I/O.

use fantastech_protocol::constants::{PRODUCT_ID_AK110_DS, PRODUCT_ID_MK894, REPORT_LENGTH};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use crate::{Transport, TransportError};

const VENDOR_ID_SDCX: u16 = 0x36AE;

const DIGCF_PRESENT: u32 = 0x00000002;
const DIGCF_DEVICEINTERFACE: u32 = 0x00000010;

const GENERIC_READ: u32 = 0x80000000;
const GENERIC_WRITE: u32 = 0x40000000;
const FILE_SHARE_READ: u32 = 0x00000001;
const FILE_SHARE_WRITE: u32 = 0x00000002;
const OPEN_EXISTING: u32 = 3;
#[allow(dead_code)]
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
const FILE_FLAG_OVERLAPPED: u32 = 0x40000000;

const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;
const ERROR_IO_PENDING: u32 = 997;
const WAIT_OBJECT_0: u32 = 0;

#[repr(C)]
struct GUID {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

#[repr(C)]
struct SP_DEVICE_INTERFACE_DATA {
    cb_size: u32,
    interface_class_guid: GUID,
    flags: u32,
    reserved: usize,
}

#[repr(C)]
struct OVERLAPPED {
    internal: usize,
    internal_high: usize,
    offset: u32,
    offset_high: u32,
    h_event: *mut std::ffi::c_void,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct HidpCaps {
    pub usage: u16,
    pub usage_page: u16,
    pub input_report_byte_length: u16,
    pub output_report_byte_length: u16,
    pub feature_report_byte_length: u16,
    pub reserved: [u16; 17],
    pub number_link_collection_nodes: u16,
    pub number_input_button_caps: u16,
    pub number_input_value_caps: u16,
    pub number_input_data_indices: u16,
    pub number_output_button_caps: u16,
    pub number_output_value_caps: u16,
    pub number_output_data_indices: u16,
    pub number_feature_button_caps: u16,
    pub number_feature_value_caps: u16,
    pub number_feature_data_indices: u16,
}

#[link(name = "setupapi")]
#[link(name = "hid")]
#[link(name = "kernel32")]
extern "system" {
    fn HidD_GetHidGuid(hid_guid: *mut GUID);
    fn SetupDiGetClassDevsW(
        class_guid: *const GUID,
        enumerator: *const u16,
        hwnd_parent: *mut std::ffi::c_void,
        flags: u32,
    ) -> *mut std::ffi::c_void;
    fn SetupDiEnumDeviceInterfaces(
        device_info_set: *mut std::ffi::c_void,
        device_info_data: *mut std::ffi::c_void,
        interface_class_guid: *const GUID,
        member_index: u32,
        device_interface_data: *mut SP_DEVICE_INTERFACE_DATA,
    ) -> i32;
    fn SetupDiGetDeviceInterfaceDetailW(
        device_info_set: *mut std::ffi::c_void,
        device_interface_data: *mut SP_DEVICE_INTERFACE_DATA,
        device_interface_detail_data: *mut u8,
        device_interface_detail_data_size: u32,
        required_size: *mut u32,
        device_info_data: *mut std::ffi::c_void,
    ) -> i32;
    fn SetupDiDestroyDeviceInfoList(device_info_set: *mut std::ffi::c_void) -> i32;

    fn HidD_GetPreparsedData(
        hid_device_object: *mut std::ffi::c_void,
        preparsed_data: *mut *mut std::ffi::c_void,
    ) -> i32;
    fn HidD_FreePreparsedData(preparsed_data: *mut std::ffi::c_void) -> i32;
    fn HidP_GetCaps(
        preparsed_data: *mut std::ffi::c_void,
        capabilities: *mut HidpCaps,
    ) -> i32;
    fn HidD_SetOutputReport(
        hid_device_object: *mut std::ffi::c_void,
        report_buffer: *const u8,
        report_buffer_length: u32,
    ) -> i32;
    fn HidD_GetInputReport(
        hid_device_object: *mut std::ffi::c_void,
        report_buffer: *mut u8,
        report_buffer_length: u32,
    ) -> i32;
    fn HidD_SetFeature(
        hid_device_object: *mut std::ffi::c_void,
        report_buffer: *const u8,
        report_buffer_length: u32,
    ) -> i32;
    fn HidD_GetFeature(
        hid_device_object: *mut std::ffi::c_void,
        report_buffer: *mut u8,
        report_buffer_length: u32,
    ) -> i32;

    fn CreateFileW(
        file_name: *const u16,
        desired_access: u32,
        share_mode: u32,
        security_attributes: *mut std::ffi::c_void,
        creation_disposition: u32,
        flags_and_attributes: u32,
        template_file: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;
    fn ReadFile(
        file: *mut std::ffi::c_void,
        buffer: *mut u8,
        number_of_bytes_to_read: u32,
        number_of_bytes_read: *mut u32,
        overlapped: *mut OVERLAPPED,
    ) -> i32;
    fn WriteFile(
        file: *mut std::ffi::c_void,
        buffer: *const u8,
        number_of_bytes_to_write: u32,
        number_of_bytes_written: *mut u32,
        overlapped: *mut OVERLAPPED,
    ) -> i32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    fn CreateEventW(
        event_attributes: *mut std::ffi::c_void,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> *mut std::ffi::c_void;
    fn WaitForSingleObject(handle: *mut std::ffi::c_void, milliseconds: u32) -> u32;
    fn GetOverlappedResult(
        file: *mut std::ffi::c_void,
        overlapped: *mut OVERLAPPED,
        number_of_bytes_transferred: *mut u32,
        wait: i32,
    ) -> i32;
    fn CancelIoEx(file: *mut std::ffi::c_void, overlapped: *mut OVERLAPPED) -> i32;
    fn GetLastError() -> u32;
}

/// Windows native USB HID hardware transport.
pub struct HidrawTransport {
    handle: *mut std::ffi::c_void,
    path: PathBuf,
    output_report_len: u16,
    input_report_len: u16,
    #[allow(dead_code)]
    feature_report_len: u16,
}

#[allow(unsafe_code)]
unsafe impl Send for HidrawTransport {}
#[allow(unsafe_code)]
unsafe impl Sync for HidrawTransport {}

impl Drop for HidrawTransport {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        if !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

#[allow(unsafe_code)]
unsafe fn query_caps(handle: *mut std::ffi::c_void) -> Option<HidpCaps> {
    let mut preparsed = std::ptr::null_mut();
    if HidD_GetPreparsedData(handle, &mut preparsed) == 0 || preparsed.is_null() {
        return None;
    }
    let mut caps: HidpCaps = std::mem::zeroed();
    let res = HidP_GetCaps(preparsed, &mut caps);
    HidD_FreePreparsedData(preparsed);
    if res == 0x00110000 || res == 1 || caps.output_report_byte_length > 0 || caps.input_report_byte_length > 0 {
        Some(caps)
    } else {
        None
    }
}

impl HidrawTransport {
    /// Opens an explicit device path.
    #[allow(unsafe_code)]
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, TransportError> {
        use std::os::windows::ffi::OsStrExt;
        let p = path.as_ref().to_path_buf();
        let path_wide: Vec<u16> = path.as_ref().as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();

        // 1. Try Read/Write with standard sharing
        let mut handle = unsafe {
            CreateFileW(
                path_wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                std::ptr::null_mut(),
            )
        };

        // 2. If access denied (error 5), attempt opening with GENERIC_WRITE only
        if handle == INVALID_HANDLE_VALUE && unsafe { GetLastError() } == 5 {
            handle = unsafe {
                CreateFileW(
                    path_wide.as_ptr(),
                    GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null_mut(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    std::ptr::null_mut(),
                )
            };
        }

        // 3. Fallback to query access (0) for HidD_* APIs if both failed
        if handle == INVALID_HANDLE_VALUE && unsafe { GetLastError() } == 5 {
            handle = unsafe {
                CreateFileW(
                    path_wide.as_ptr(),
                    0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null_mut(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                )
            };
        }

        if handle == INVALID_HANDLE_VALUE {
            let err = unsafe { GetLastError() };
            if err == 5 {
                return Err(TransportError::PermissionDenied(p));
            }
            return Err(TransportError::Io(std::io::Error::from_raw_os_error(err as i32)));
        }

        let (output_report_len, input_report_len, feature_report_len) = if let Some(caps) = unsafe { query_caps(handle) } {
            (
                if caps.output_report_byte_length > 0 { caps.output_report_byte_length } else { 65 },
                if caps.input_report_byte_length > 0 { caps.input_report_byte_length } else { 65 },
                caps.feature_report_byte_length,
            )
        } else {
            (65, 65, 0)
        };

        Ok(Self {
            handle,
            path: p,
            output_report_len,
            input_report_len,
            feature_report_len,
        })
    }

    /// Enumerates Windows HID devices and finds the Fantech MK894 Vendor Interface (MI_02).
    pub fn discover() -> Result<Self, TransportError> {
        let vid_str = format!("vid_{:04x}", VENDOR_ID_SDCX);
        let pid_mk894 = format!("pid_{:04x}", PRODUCT_ID_MK894);
        let pid_ak110 = format!("pid_{:04x}", PRODUCT_ID_AK110_DS);

        let paths = enumerate_hid_device_paths();
        tracing::debug!("Found {} Windows HID device paths", paths.len());

        let mut primary_candidates = Vec::new();
        let mut fallback_candidates = Vec::new();

        for dev_path in paths {
            let lower = dev_path.to_lowercase();
            if lower.contains(&vid_str) && (lower.contains(&pid_mk894) || lower.contains(&pid_ak110)) {
                // Interface 0 (mi_00): Standard keyboard (locked by Windows kbdclass.sys)
                // Interface 1 (mi_01): Media keys (locked by Windows media key driver)
                // Interface 2 (mi_02): Vendor Control Interface (open for read/write)
                if lower.contains("mi_02") || lower.contains("&mi_2") {
                    primary_candidates.push(dev_path);
                } else if !lower.contains("mi_00") && !lower.contains("mi_01") {
                    fallback_candidates.push(dev_path);
                } else if lower.contains("mi_01") {
                    fallback_candidates.push(dev_path);
                }
            }
        }

        let mut last_err = None;

        // Try primary interface 2 (MI_02) candidates first
        for candidate in primary_candidates {
            match Self::open(&candidate) {
                Ok(transport) => {
                    tracing::info!("Connected to Fantech MK894 Vendor Interface (MI_02) at: {}", candidate);
                    return Ok(transport);
                }
                Err(e) => {
                    tracing::warn!("Failed opening candidate {}: {}", candidate, e);
                    last_err = Some(e);
                }
            }
        }

        // Try fallback candidates
        for candidate in fallback_candidates {
            match Self::open(&candidate) {
                Ok(transport) => {
                    tracing::info!("Connected to Fantech MK894 candidate interface at: {}", candidate);
                    return Ok(transport);
                }
                Err(e) => {
                    tracing::warn!("Failed opening fallback {}: {}", candidate, e);
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or(TransportError::DeviceNotFound))
    }

    /// Automatically locates the IAP Bootloader device (PID 0xFEAB).
    pub fn discover_bootloader() -> Result<Self, TransportError> {
        let vid_str = format!("vid_{:04x}", VENDOR_ID_SDCX);
        let pid_bootloader = format!("pid_{:04x}", PRODUCT_ID_AK110_DS);

        let paths = enumerate_hid_device_paths();
        for dev_path in paths {
            let lower = dev_path.to_lowercase();
            if lower.contains(&vid_str) && (lower.contains(&pid_bootloader) || lower.contains("feab")) {
                if let Ok(transport) = Self::open(&dev_path) {
                    tracing::info!("Discovered Fantech IAP Bootloader at: {}", dev_path);
                    return Ok(transport);
                }
            }
        }

        Err(TransportError::DeviceNotFound)
    }

    /// Discards any stale unread reports.
    pub fn drain(&mut self) {
        // Overlapped reading handles fresh reports per transaction on Windows.
    }

    /// Returns the active device node path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Polls until a device matching `target_pid` is discovered, or until timeout.
    pub fn wait_for_device(target_pid: u16, timeout: Duration) -> Result<Self, TransportError> {
        let start = Instant::now();
        loop {
            let res = if target_pid == PRODUCT_ID_AK110_DS {
                Self::discover_bootloader()
            } else {
                Self::discover()
            };
            match res {
                Ok(transport) => return Ok(transport),
                Err(TransportError::PermissionDenied(p)) => return Err(TransportError::PermissionDenied(p)),
                Err(_) => {}
            }
            if start.elapsed() >= timeout {
                return Err(TransportError::Timeout(timeout));
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

impl Transport for HidrawTransport {
    #[allow(unsafe_code)]
    fn read_report(&mut self, buf: &mut [u8; REPORT_LENGTH]) -> Result<usize, TransportError> {
        let report_len = self.input_report_len as usize;
        let mut wire_buf = [0u8; 65];
        let (read_ptr, read_len) = if report_len == 65 {
            (wire_buf.as_mut_ptr(), 65u32)
        } else if report_len == 64 {
            (buf.as_mut_ptr(), 64u32)
        } else {
            (wire_buf.as_mut_ptr(), 65u32)
        };

        let mut read_bytes: u32 = 0;
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.h_event = unsafe { CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null_mut()) };
        if ov.h_event.is_null() {
            return Err(TransportError::Io(std::io::Error::last_os_error()));
        }

        let res = unsafe {
            ReadFile(
                self.handle,
                read_ptr,
                read_len,
                &mut read_bytes,
                &mut ov,
            )
        };

        let success = if res == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_IO_PENDING {
                let wait_res = unsafe { WaitForSingleObject(ov.h_event, 2000) };
                if wait_res == WAIT_OBJECT_0 {
                    let mut transferred = 0;
                    let ok = unsafe { GetOverlappedResult(self.handle, &mut ov, &mut transferred, 0) };
                    unsafe { CloseHandle(ov.h_event) };
                    if ok != 0 {
                        true
                    } else {
                        return Err(TransportError::Io(std::io::Error::last_os_error()));
                    }
                } else {
                    unsafe {
                        CancelIoEx(self.handle, &mut ov);
                        CloseHandle(ov.h_event);
                    }
                    return Err(TransportError::Timeout(Duration::from_millis(2000)));
                }
            } else {
                unsafe { CloseHandle(ov.h_event) };
                // Fallback to HidD_GetInputReport if ReadFile is rejected by mini-driver
                let ok = unsafe { HidD_GetInputReport(self.handle, read_ptr, read_len) };
                if ok != 0 {
                    true
                } else {
                    let feat_ok = unsafe { HidD_GetFeature(self.handle, read_ptr, read_len) };
                    if feat_ok != 0 {
                        true
                    } else {
                        return Err(TransportError::Io(std::io::Error::from_raw_os_error(err as i32)));
                    }
                }
            }
        } else {
            unsafe { CloseHandle(ov.h_event) };
            true
        };

        if success {
            if read_len == 65 {
                // If the first byte is Report ID 0x00, payload starts at offset 1
                if wire_buf[0] == 0x00 {
                    buf.copy_from_slice(&wire_buf[1..65]);
                } else {
                    // Otherwise payload started at offset 0
                    buf.copy_from_slice(&wire_buf[0..64]);
                }
            }
            tracing::debug!("[WIN HID RX] ({} bytes): {:02X?}", REPORT_LENGTH, &buf[..REPORT_LENGTH.min(16)]);
            Ok(REPORT_LENGTH)
        } else {
            Err(TransportError::Timeout(Duration::from_millis(2000)))
        }
    }

    #[allow(unsafe_code)]
    fn write_report(&mut self, buf: &[u8; REPORT_LENGTH]) -> Result<(), TransportError> {
        tracing::debug!("[WIN HID TX] ({} bytes): {:02X?}", buf.len(), &buf[..16]);

        let report_len = self.output_report_len as usize;
        let mut wire_buf = [0u8; 65];
        let (send_ptr, send_len) = if report_len == 65 {
            // Unnumbered HID report: Byte 0 is Report ID 0x00, bytes 1..65 are payload
            wire_buf[0] = 0x00;
            wire_buf[1..65].copy_from_slice(buf);
            (wire_buf.as_ptr(), 65u32)
        } else if report_len == 64 {
            (buf.as_ptr(), 64u32)
        } else {
            wire_buf[0] = 0x00;
            wire_buf[1..65].copy_from_slice(buf);
            (wire_buf.as_ptr(), 65u32)
        };

        let mut written: u32 = 0;
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.h_event = unsafe { CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null_mut()) };
        if ov.h_event.is_null() {
            return Err(TransportError::Io(std::io::Error::last_os_error()));
        }

        let res = unsafe {
            WriteFile(
                self.handle,
                send_ptr,
                send_len,
                &mut written,
                &mut ov,
            )
        };

        if res == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_IO_PENDING {
                let wait_res = unsafe { WaitForSingleObject(ov.h_event, 2000) };
                if wait_res == WAIT_OBJECT_0 {
                    let mut transferred = 0;
                    let ok = unsafe { GetOverlappedResult(self.handle, &mut ov, &mut transferred, 0) };
                    unsafe { CloseHandle(ov.h_event) };
                    if ok != 0 {
                        return Ok(());
                    }
                    return Err(TransportError::Io(std::io::Error::last_os_error()));
                } else {
                    unsafe {
                        CancelIoEx(self.handle, &mut ov);
                        CloseHandle(ov.h_event);
                    }
                    // Try HidD_SetOutputReport as fallback
                    let fallback_ok = unsafe { HidD_SetOutputReport(self.handle, send_ptr, send_len) };
                    if fallback_ok != 0 {
                        return Ok(());
                    }
                    let feat_ok = unsafe { HidD_SetFeature(self.handle, send_ptr, send_len) };
                    if feat_ok != 0 {
                        return Ok(());
                    }
                    return Err(TransportError::Timeout(Duration::from_millis(2000)));
                }
            } else {
                unsafe { CloseHandle(ov.h_event) };
                // Try HidD_SetOutputReport as fallback
                let fallback_ok = unsafe { HidD_SetOutputReport(self.handle, send_ptr, send_len) };
                if fallback_ok != 0 {
                    return Ok(());
                }
                // Try HidD_SetFeature as fallback
                let feat_ok = unsafe { HidD_SetFeature(self.handle, send_ptr, send_len) };
                if feat_ok != 0 {
                    return Ok(());
                }
                return Err(TransportError::Io(std::io::Error::from_raw_os_error(err as i32)));
            }
        }

        unsafe { CloseHandle(ov.h_event) };
        Ok(())
    }

    fn reconnect(&mut self, target_pid: u16, timeout: Duration) -> Result<(), TransportError> {
        let mut new_dev = std::mem::ManuallyDrop::new(Self::wait_for_device(target_pid, timeout)?);
        if !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE {
            #[allow(unsafe_code)]
            unsafe {
                CloseHandle(self.handle);
            }
        }
        self.handle = new_dev.handle;
        self.path = std::mem::take(&mut new_dev.path);
        self.output_report_len = new_dev.output_report_len;
        self.input_report_len = new_dev.input_report_len;
        self.feature_report_len = new_dev.feature_report_len;
        Ok(())
    }
}

/// Helper function to enumerate all connected HID device interface paths via SetupAPI.
#[allow(unsafe_code)]
fn enumerate_hid_device_paths() -> Vec<String> {
    let mut results = Vec::new();

    let mut hid_guid: GUID = unsafe { std::mem::zeroed() };
    unsafe {
        HidD_GetHidGuid(&mut hid_guid);
    }

    let dev_info = unsafe {
        SetupDiGetClassDevsW(
            &hid_guid,
            std::ptr::null(),
            std::ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };

    if dev_info.is_null() || dev_info == INVALID_HANDLE_VALUE {
        return results;
    }

    let mut index = 0;
    loop {
        let mut iface_data: SP_DEVICE_INTERFACE_DATA = unsafe { std::mem::zeroed() };
        iface_data.cb_size = std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;

        let ok = unsafe {
            SetupDiEnumDeviceInterfaces(
                dev_info,
                std::ptr::null_mut(),
                &hid_guid,
                index,
                &mut iface_data,
            )
        };

        if ok == 0 {
            break;
        }

        // Determine required buffer size
        let mut req_size: u32 = 0;
        unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                dev_info,
                &mut iface_data,
                std::ptr::null_mut(),
                0,
                &mut req_size,
                std::ptr::null_mut(),
            );
        }

        if req_size > 0 {
            // cbSize is 8 on 64-bit Windows, 5/6 on 32-bit
            let cb_size = if std::mem::size_of::<usize>() == 8 { 8u32 } else { 5u32 };
            let mut buffer: Vec<u8> = vec![0u8; req_size as usize];
            buffer[0..4].copy_from_slice(&cb_size.to_ne_bytes());

            let ok = unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    dev_info,
                    &mut iface_data,
                    buffer.as_mut_ptr(),
                    req_size,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };

            if ok != 0 {
                // DevicePath is null-terminated UTF-16 starting at offset 4
                let path_u16_slice: &[u16] = unsafe {
                    let ptr = buffer.as_ptr().add(4) as *const u16;
                    let len = (req_size as usize - 4) / 2;
                    std::slice::from_raw_parts(ptr, len)
                };

                let null_pos = path_u16_slice.iter().position(|&c| c == 0).unwrap_or(path_u16_slice.len());
                let dev_path = String::from_utf16_lossy(&path_u16_slice[..null_pos]);
                results.push(dev_path);
            }
        }

        index += 1;
    }

    unsafe {
        SetupDiDestroyDeviceInfoList(dev_info);
    }

    results
}
