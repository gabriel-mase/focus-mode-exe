use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub device_name: String, // e.g. r"\\.\DISPLAY1"
    pub friendly_name: String,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    pub is_primary: bool,
}

// ── Constants ──────────────────────────────────────────────────────────────────

// EnumDisplayDevices
const DISPLAY_DEVICE_ACTIVE: u32 = 0x0000_0001;
const DISPLAY_DEVICE_PRIMARY_DEVICE: u32 = 0x0000_0004;
const ENUM_CURRENT_SETTINGS: u32 = 0xFFFF_FFFF;

// QueryDisplayConfig / SetDisplayConfig (CCD API)
const QDC_ONLY_ACTIVE_PATHS: u32 = 0x0000_0002;
const SDC_USE_SUPPLIED_DISPLAY_CONFIG: u32 = 0x0000_0020;
const SDC_APPLY: u32 = 0x0000_0080;
const SDC_ALLOW_CHANGES: u32 = 0x0000_0400;

// DisplayConfigGetDeviceInfo type
const DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME: i32 = 1;

// ── Public API ─────────────────────────────────────────────────────────────────

pub fn enumerate_monitors() -> Vec<MonitorInfo> {
    #[cfg(windows)]
    return enumerate_win32();
    #[cfg(not(windows))]
    return vec![];
}

pub fn disable_non_primary_monitors() {
    // DisplaySwitch.exe /internal = "PC screen only" — proven, reliable path.
    #[cfg(windows)]
    let _ = std::process::Command::new("DisplaySwitch.exe")
        .arg("/internal")
        .spawn();
}

pub fn disable_specific_monitors(device_names: &[String]) {
    if device_names.is_empty() {
        disable_non_primary_monitors();
    } else {
        // Use the Windows CCD API (SetDisplayConfig) which is what
        // DisplaySwitch.exe itself uses under the hood.
        #[cfg(windows)]
        disable_via_ccd(device_names);
    }
}

pub fn restore_all_monitors() {
    // DisplaySwitch.exe /extend re-enables all connected monitors.
    #[cfg(windows)]
    let _ = std::process::Command::new("DisplaySwitch.exe")
        .arg("/extend")
        .spawn();
}

/// Opaque snapshot of the active Windows display configuration, captured
/// before a game starts so the exact layout can be restored on game close.
/// Using `DisplaySwitch.exe /extend` for restore is unreliable when specific
/// monitors were disabled via CCD without SDC_SAVE_TO_DATABASE, because
/// /extend may apply "extend" only over the currently-active monitors rather
/// than the full pre-game set.
#[cfg(windows)]
pub struct SavedDisplayConfig {
    paths: Vec<ffi::DisplayConfigPathInfo>,
    modes: Vec<ffi::DisplayConfigModeInfo>,
}

// SAFETY: the FFI structs contain only integer primitives — no raw pointers.
#[cfg(windows)]
unsafe impl Send for SavedDisplayConfig {}

#[cfg(not(windows))]
pub struct SavedDisplayConfig;

/// Capture the current active display configuration.
pub fn capture_display_config() -> Option<SavedDisplayConfig> {
    #[cfg(windows)]
    return capture_win32();
    #[cfg(not(windows))]
    return None;
}

/// Restore a previously captured display configuration.
pub fn restore_saved_config(config: SavedDisplayConfig) {
    #[cfg(windows)]
    restore_win32(config);
    #[cfg(not(windows))]
    let _ = config;
}

// ── FFI ────────────────────────────────────────────────────────────────────────

#[cfg(windows)]
mod ffi {
    use std::os::raw::c_long;
    pub type DWORD = u32;
    pub type BOOL = i32;
    pub type LONG = c_long;

    // ── EnumDisplayDevices structs ─────────────────────────────────────────────

    #[repr(C)]
    pub struct DisplayDeviceW {
        pub cb: DWORD,
        pub device_name: [u16; 32],
        pub device_string: [u16; 128],
        pub state_flags: DWORD,
        pub device_id: [u16; 128],
        pub device_key: [u16; 128],
    }

    #[repr(C)]
    pub struct DevModeW {
        pub dm_device_name: [u16; 32],
        pub dm_spec_version: u16,
        pub dm_driver_version: u16,
        pub dm_size: u16,
        pub dm_driver_extra: u16,
        pub dm_fields: DWORD,
        pub _union1: [u8; 16],
        pub dm_color: i16,
        pub dm_duplex: i16,
        pub dm_y_resolution: i16,
        pub dm_tt_option: i16,
        pub dm_collate: i16,
        pub dm_form_name: [u16; 32],
        pub dm_log_pixels: u16,
        pub dm_bits_per_pel: DWORD,
        pub dm_pels_width: DWORD,
        pub dm_pels_height: DWORD,
        pub _union2: [u8; 4],
        pub dm_display_frequency: DWORD,
        pub dm_icm_method: DWORD,
        pub dm_icm_intent: DWORD,
        pub dm_media_type: DWORD,
        pub dm_dither_type: DWORD,
        pub dm_reserved1: DWORD,
        pub dm_reserved2: DWORD,
        pub dm_panning_width: DWORD,
        pub dm_panning_height: DWORD,
    }

    // ── CCD (SetDisplayConfig) structs ─────────────────────────────────────────
    //
    // Sizes verified against Windows SDK headers:
    //   LUID                           =  8 bytes
    //   DISPLAYCONFIG_RATIONAL         =  8 bytes
    //   DISPLAYCONFIG_PATH_SOURCE_INFO = 20 bytes
    //   DISPLAYCONFIG_PATH_TARGET_INFO = 48 bytes
    //   DISPLAYCONFIG_PATH_INFO        = 72 bytes
    //   DISPLAYCONFIG_MODE_INFO        = 64 bytes
    //   DISPLAYCONFIG_DEVICE_INFO_HEADER = 20 bytes
    //   DISPLAYCONFIG_SOURCE_DEVICE_NAME = 84 bytes

    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct Luid {
        pub low_part: u32,  // LowPart
        pub high_part: i32, // HighPart
    }

    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct DisplayConfigRational {
        pub numerator: u32,
        pub denominator: u32,
    }

    /// DISPLAYCONFIG_PATH_SOURCE_INFO (20 bytes)
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct DisplayConfigPathSourceInfo {
        pub adapter_id: Luid,   // 8
        pub id: u32,             // 4
        pub mode_info_idx: u32,  // 4  (union modeInfoIdx / bitfields — u32 covers both)
        pub status_flags: u32,   // 4
    }

    /// DISPLAYCONFIG_PATH_TARGET_INFO (48 bytes)
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct DisplayConfigPathTargetInfo {
        pub adapter_id: Luid,                    // 8
        pub id: u32,                              // 4
        pub mode_info_idx: u32,                   // 4
        pub output_technology: u32,               // 4
        pub rotation: u32,                        // 4
        pub scaling: u32,                         // 4
        pub refresh_rate: DisplayConfigRational,  // 8
        pub scan_line_ordering: u32,              // 4
        pub target_available: BOOL,               // 4
        pub status_flags: u32,                    // 4
    }

    /// DISPLAYCONFIG_PATH_INFO (72 bytes)
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct DisplayConfigPathInfo {
        pub source_info: DisplayConfigPathSourceInfo, // 20
        pub target_info: DisplayConfigPathTargetInfo, // 48
        pub flags: u32,                                // 4
    }

    /// DISPLAYCONFIG_MODE_INFO (64 bytes).
    /// The union (targetMode / sourceMode / desktopImageInfo) is 48 bytes;
    /// we store it as [u64; 6] to guarantee 8-byte alignment.
    #[derive(Clone, Copy)]
    #[repr(C, align(8))]
    pub struct DisplayConfigModeInfo {
        pub info_type: u32,   // 4
        pub id: u32,           // 4
        pub adapter_id: Luid,  // 8
        pub _union: [u64; 6],  // 48  (covers all three union variants)
    }

    /// DISPLAYCONFIG_DEVICE_INFO_HEADER (20 bytes)
    #[repr(C)]
    pub struct DisplayConfigDeviceInfoHeader {
        pub info_type: i32,   // DISPLAYCONFIG_DEVICE_INFO_TYPE  4
        pub size: u32,         //                                  4
        pub adapter_id: Luid,  //                                  8
        pub id: u32,           //                                  4
    }

    /// DISPLAYCONFIG_SOURCE_DEVICE_NAME (84 bytes).
    /// Returned by DisplayConfigGetDeviceInfo; contains the GDI device name
    /// (e.g. "\\\\.\\DISPLAY2") that matches EnumDisplayDevicesW output.
    #[repr(C)]
    pub struct DisplayConfigSourceDeviceName {
        pub header: DisplayConfigDeviceInfoHeader, // 20
        pub view_gdi_device_name: [u16; 32],       // 64  (CCHDEVICENAME = 32 WCHARs)
    }

    #[link(name = "user32")]
    extern "system" {
        // ── Enumerate monitors ──────────────────────────────────────────────────
        pub fn EnumDisplayDevicesW(
            lp_device: *const u16,
            i_dev_num: DWORD,
            lp_display_device: *mut DisplayDeviceW,
            dw_flags: DWORD,
        ) -> BOOL;

        pub fn EnumDisplaySettingsW(
            lpsz_device_name: *const u16,
            i_mode_num: DWORD,
            lp_dev_mode: *mut DevModeW,
        ) -> BOOL;

        // ── CCD API ────────────────────────────────────────────────────────────
        pub fn GetDisplayConfigBufferSizes(
            flags: u32,
            num_path_array_elements: *mut u32,
            num_mode_info_array_elements: *mut u32,
        ) -> LONG;

        pub fn QueryDisplayConfig(
            flags: u32,
            num_path_array_elements: *mut u32,
            path_array: *mut DisplayConfigPathInfo,
            num_mode_info_array_elements: *mut u32,
            mode_info_array: *mut DisplayConfigModeInfo,
            current_topology_id: *mut u32,
        ) -> LONG;

        pub fn SetDisplayConfig(
            num_path_array_elements: u32,
            path_array: *mut DisplayConfigPathInfo,
            num_mode_info_array_elements: u32,
            mode_info_array: *mut DisplayConfigModeInfo,
            flags: u32,
        ) -> LONG;

        pub fn DisplayConfigGetDeviceInfo(
            request_packet: *mut DisplayConfigDeviceInfoHeader,
        ) -> LONG;
    }
}

// ── Windows implementation ─────────────────────────────────────────────────────

#[cfg(windows)]
fn enumerate_win32() -> Vec<MonitorInfo> {
    use std::mem;
    let mut monitors = Vec::new();
    let mut idx: u32 = 0;

    loop {
        let mut dd: ffi::DisplayDeviceW = unsafe { mem::zeroed() };
        dd.cb = mem::size_of::<ffi::DisplayDeviceW>() as u32;

        let ok = unsafe { ffi::EnumDisplayDevicesW(std::ptr::null(), idx, &mut dd, 0) };
        if ok == 0 {
            break;
        }
        idx += 1;

        if dd.state_flags & DISPLAY_DEVICE_ACTIVE == 0 {
            continue;
        }

        let device_name = wstr_to_string(&dd.device_name);
        let name_wide = to_wide(&device_name);

        let mut dm: ffi::DevModeW = unsafe { mem::zeroed() };
        dm.dm_size = mem::size_of::<ffi::DevModeW>() as u16;
        unsafe {
            ffi::EnumDisplaySettingsW(name_wide.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm);
        }

        // Try to get a friendly name via the second EnumDisplayDevicesW call
        let mut monitor_dd: ffi::DisplayDeviceW = unsafe { mem::zeroed() };
        monitor_dd.cb = mem::size_of::<ffi::DisplayDeviceW>() as u32;
        let has_monitor =
            unsafe { ffi::EnumDisplayDevicesW(name_wide.as_ptr(), 0, &mut monitor_dd, 0) } != 0;

        let friendly_name = if has_monitor && monitor_dd.device_string[0] != 0 {
            wstr_to_string(&monitor_dd.device_string)
        } else {
            wstr_to_string(&dd.device_string)
        };

        monitors.push(MonitorInfo {
            device_name,
            friendly_name,
            width: dm.dm_pels_width,
            height: dm.dm_pels_height,
            refresh_hz: dm.dm_display_frequency,
            is_primary: dd.state_flags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0,
        });
    }

    monitors
}

/// Disable specific monitors using the Windows CCD API (SetDisplayConfig).
/// This is the same API used internally by DisplaySwitch.exe, which is why it
/// works reliably on Windows 10/11 where ChangeDisplaySettingsEx(0×0) does not.
///
/// Strategy:
///   1. QueryDisplayConfig  — get all currently active display paths
///   2. For each path, call DisplayConfigGetDeviceInfo to get its GDI device name
///      (e.g. "\\\\.\\DISPLAY2"), and filter out paths whose device name is in
///      the to-disable list
///   3. SetDisplayConfig with only the remaining (to-keep) paths — Windows turns
///      off any monitor whose path is absent
///   4. Restore is always handled by DisplaySwitch.exe /extend
#[cfg(windows)]
fn disable_via_ccd(device_names_to_disable: &[String]) {
    use std::mem;
    unsafe {
        // Step 1 — allocate buffers
        let mut num_paths: u32 = 0;
        let mut num_modes: u32 = 0;
        if ffi::GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut num_paths, &mut num_modes)
            != 0
        {
            return;
        }

        let mut paths =
            vec![mem::zeroed::<ffi::DisplayConfigPathInfo>(); num_paths as usize];
        let mut modes =
            vec![mem::zeroed::<ffi::DisplayConfigModeInfo>(); num_modes as usize];

        // Step 2 — query active paths
        if ffi::QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut num_paths,
            paths.as_mut_ptr(),
            &mut num_modes,
            modes.as_mut_ptr(),
            std::ptr::null_mut(),
        ) != 0
        {
            return;
        }
        paths.truncate(num_paths as usize);
        modes.truncate(num_modes as usize);

        // Step 3 — build the "keep" list by resolving GDI device names
        let mut paths_to_keep: Vec<ffi::DisplayConfigPathInfo> = Vec::new();
        for path in &paths {
            let mut sdn = mem::zeroed::<ffi::DisplayConfigSourceDeviceName>();
            sdn.header.info_type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            sdn.header.size =
                mem::size_of::<ffi::DisplayConfigSourceDeviceName>() as u32;
            sdn.header.adapter_id = path.source_info.adapter_id;
            sdn.header.id = path.source_info.id;

            let gdi_name = if ffi::DisplayConfigGetDeviceInfo(
                // Cast: DisplayConfigSourceDeviceName starts with the header field,
                // so &sdn == &sdn.header at the same address.
                &mut sdn as *mut ffi::DisplayConfigSourceDeviceName
                    as *mut ffi::DisplayConfigDeviceInfoHeader,
            ) == 0
            {
                wstr_to_string(&sdn.view_gdi_device_name)
            } else {
                String::new() // can't identify — keep this path
            };

            if !gdi_name.is_empty() && device_names_to_disable.contains(&gdi_name) {
                // Omit from paths_to_keep → Windows will disable this monitor
            } else {
                paths_to_keep.push(*path);
            }
        }

        if paths_to_keep.len() == paths.len() {
            // No matching device found — nothing to disable
            return;
        }

        // Step 4 — apply new topology (monitors not in paths_to_keep are disabled).
        // Intentionally NOT using SDC_SAVE_TO_DATABASE: the disable is session-only
        // so the "extend" topology in the database stays clean, allowing
        // DisplaySwitch.exe /extend to restore all monitors correctly on game close.
        ffi::SetDisplayConfig(
            paths_to_keep.len() as u32,
            paths_to_keep.as_mut_ptr(),
            modes.len() as u32,
            modes.as_mut_ptr(),
            SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES,
        );
    }
}

// ── Capture / restore display config ──────────────────────────────────────────

#[cfg(windows)]
fn capture_win32() -> Option<SavedDisplayConfig> {
    use std::mem;
    unsafe {
        let mut num_paths: u32 = 0;
        let mut num_modes: u32 = 0;
        if ffi::GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut num_paths, &mut num_modes)
            != 0
        {
            return None;
        }
        let mut paths =
            vec![mem::zeroed::<ffi::DisplayConfigPathInfo>(); num_paths as usize];
        let mut modes =
            vec![mem::zeroed::<ffi::DisplayConfigModeInfo>(); num_modes as usize];
        if ffi::QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut num_paths,
            paths.as_mut_ptr(),
            &mut num_modes,
            modes.as_mut_ptr(),
            std::ptr::null_mut(),
        ) != 0
        {
            return None;
        }
        paths.truncate(num_paths as usize);
        modes.truncate(num_modes as usize);
        Some(SavedDisplayConfig { paths, modes })
    }
}

#[cfg(windows)]
fn restore_win32(mut config: SavedDisplayConfig) {
    unsafe {
        ffi::SetDisplayConfig(
            config.paths.len() as u32,
            config.paths.as_mut_ptr(),
            config.modes.len() as u32,
            config.modes.as_mut_ptr(),
            SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_ALLOW_CHANGES,
        );
    }
}

// ── String helpers ─────────────────────────────────────────────────────────────

fn wstr_to_string(arr: &[u16]) -> String {
    let end = arr.iter().position(|&c| c == 0).unwrap_or(arr.len());
    String::from_utf16_lossy(&arr[..end])
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
