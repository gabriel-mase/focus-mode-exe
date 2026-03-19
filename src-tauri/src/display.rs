use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub device_name: String,   // e.g. r"\\.\DISPLAY1"
    pub friendly_name: String,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    pub is_primary: bool,
}

// ── Win32 constants ────────────────────────────────────────────────────────────
const DISPLAY_DEVICE_ACTIVE: u32 = 0x00000001;
const DISPLAY_DEVICE_PRIMARY_DEVICE: u32 = 0x00000004;
const ENUM_CURRENT_SETTINGS: u32 = 0xFFFF_FFFF;
const ENUM_REGISTRY_SETTINGS: u32 = 0xFFFF_FFFE;
const CDS_UPDATEREGISTRY: u32 = 0x0000_0001;
const CDS_NORESET: u32 = 0x1000_0000;
const DM_PELSWIDTH: u32 = 0x0008_0000;
const DM_PELSHEIGHT: u32 = 0x0010_0000;
const DM_DISPLAYFREQUENCY: u32 = 0x0040_0000;

// Saved (width, height, refresh_hz) per device before we disabled it
#[cfg(windows)]
fn saved_modes(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, (u32, u32, u32)>> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static S: OnceLock<Mutex<HashMap<String, (u32, u32, u32)>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

// ── Public API ─────────────────────────────────────────────────────────────────

pub fn enumerate_monitors() -> Vec<MonitorInfo> {
    #[cfg(windows)]
    return enumerate_win32();
    #[cfg(not(windows))]
    return vec![];
}

pub fn disable_non_primary_monitors() {
    // DisplaySwitch.exe /internal = "PC screen only" — disables all secondary
    // monitors. This is the proven, reliable path for the default case.
    #[cfg(windows)]
    let _ = std::process::Command::new("DisplaySwitch.exe")
        .arg("/internal")
        .spawn();
}

pub fn disable_specific_monitors(device_names: &[String]) {
    if device_names.is_empty() {
        disable_non_primary_monitors();
    } else {
        // Per-monitor Win32 path (used when the user configured specific monitors
        // per game). Saves original settings before disabling so restore works.
        disable_and_apply(device_names);
    }
}

pub fn restore_all_monitors() {
    // If specific monitors were disabled via Win32, restore their registry entries
    // first so the values left in the registry are clean (not 0×0).
    #[cfg(windows)]
    {
        let names: Vec<String> = saved_modes().lock().unwrap().keys().cloned().collect();
        if !names.is_empty() {
            restore_and_apply(&names);
        }
    }

    // DisplaySwitch.exe /extend = "Extend" — re-enables all connected monitors.
    #[cfg(windows)]
    let _ = std::process::Command::new("DisplaySwitch.exe")
        .arg("/extend")
        .spawn();
}

// ── Windows implementation ─────────────────────────────────────────────────────

#[cfg(windows)]
mod ffi {
    //! Raw FFI bindings. We declare the structs and functions ourselves to avoid
    //! fighting winapi's feature-gated module layout.

    use std::os::raw::c_long;

    pub type DWORD = u32;
    pub type BOOL = i32;
    pub type LONG = c_long;

    // DISPLAY_DEVICEW — declared in winuser.h, lives in user32.dll
    #[repr(C)]
    pub struct DisplayDeviceW {
        pub cb: DWORD,
        pub device_name: [u16; 32],
        pub device_string: [u16; 128],
        pub state_flags: DWORD,
        pub device_id: [u16; 128],
        pub device_key: [u16; 128],
    }

    // DEVMODEW — simplified version with fields we actually need.
    // The real struct has unions at the top; we pad those with bytes.
    // Layout from Windows SDK: first union (display vs printer) is at offset 28.
    // We only need dmFields, dmPelsWidth, dmPelsHeight, dmDisplayFrequency.
    #[repr(C)]
    pub struct DevModeW {
        pub dm_device_name: [u16; 32],   // 64 bytes
        pub dm_spec_version: u16,
        pub dm_driver_version: u16,
        pub dm_size: u16,
        pub dm_driver_extra: u16,
        pub dm_fields: DWORD,
        // Union 1: 16 bytes (display: dmPosition POINTL(8) + orientation(4) + fixed(4))
        // We just pad these since we don't use them for disabling
        pub _union1: [u8; 16],
        pub dm_color: i16,
        pub dm_duplex: i16,
        pub dm_y_resolution: i16,
        pub dm_tt_option: i16,
        pub dm_collate: i16,
        pub dm_form_name: [u16; 32],     // 64 bytes
        pub dm_log_pixels: u16,
        pub dm_bits_per_pel: DWORD,
        pub dm_pels_width: DWORD,
        pub dm_pels_height: DWORD,
        // Union 2: 4 bytes
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

    #[link(name = "user32")]
    extern "system" {
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

        pub fn ChangeDisplaySettingsExW(
            lpsz_device_name: *const u16,
            lp_dev_mode: *mut DevModeW,
            hwnd: *mut std::ffi::c_void,
            dw_flags: DWORD,
            l_param: *mut std::ffi::c_void,
        ) -> LONG;
    }
}

#[cfg(windows)]
fn enumerate_win32() -> Vec<MonitorInfo> {
    use std::mem;
    let mut monitors = Vec::new();
    let mut idx: u32 = 0;

    loop {
        let mut dd: ffi::DisplayDeviceW = unsafe { mem::zeroed() };
        dd.cb = mem::size_of::<ffi::DisplayDeviceW>() as u32;

        let ok = unsafe {
            ffi::EnumDisplayDevicesW(std::ptr::null(), idx, &mut dd, 0)
        };
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

        // Try to get friendly monitor name via second EnumDisplayDevices call
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

fn disable_and_apply(device_names: &[String]) {
    // Per-monitor disable via Win32: save current settings first so we can
    // restore them correctly, then apply 0×0 (disables the output).
    #[cfg(windows)]
    {
        use std::mem;
        let mut staged = false;
        for name in device_names {
            let name_wide = to_wide(name);

            // Read current resolution so we can put it back on restore
            let mut cur: ffi::DevModeW = unsafe { mem::zeroed() };
            cur.dm_size = mem::size_of::<ffi::DevModeW>() as u16;
            if unsafe { ffi::EnumDisplaySettingsW(name_wide.as_ptr(), ENUM_CURRENT_SETTINGS, &mut cur) } != 0 {
                saved_modes().lock().unwrap().insert(
                    name.clone(),
                    (cur.dm_pels_width, cur.dm_pels_height, cur.dm_display_frequency),
                );
            }

            // Stage the disable (width=0 height=0)
            let mut dm: ffi::DevModeW = unsafe { mem::zeroed() };
            dm.dm_size = mem::size_of::<ffi::DevModeW>() as u16;
            dm.dm_fields = DM_PELSWIDTH | DM_PELSHEIGHT;
            let ret = unsafe {
                ffi::ChangeDisplaySettingsExW(
                    name_wide.as_ptr(),
                    &mut dm,
                    std::ptr::null_mut(),
                    CDS_UPDATEREGISTRY | CDS_NORESET,
                    std::ptr::null_mut(),
                )
            };
            if ret == 0 {
                staged = true;
            }
        }
        if staged {
            // Commit all staged changes (both args NULL as MSDN requires)
            unsafe {
                ffi::ChangeDisplaySettingsExW(
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                );
            }
        }
    }
}

fn restore_and_apply(device_names: &[String]) {
    // Restore from the settings we saved before disabling.
    #[cfg(windows)]
    {
        use std::mem;
        let saved = saved_modes().lock().unwrap().clone();
        let mut staged = false;
        for name in device_names {
            let name_wide = to_wide(name);
            let mut dm: ffi::DevModeW = unsafe { mem::zeroed() };
            dm.dm_size = mem::size_of::<ffi::DevModeW>() as u16;

            // Prefer our saved values; fall back to registry if we have nothing saved
            if let Some(&(w, h, hz)) = saved.get(name) {
                dm.dm_fields = DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY;
                dm.dm_pels_width = w;
                dm.dm_pels_height = h;
                dm.dm_display_frequency = hz;
            } else {
                let ok = unsafe {
                    ffi::EnumDisplaySettingsW(name_wide.as_ptr(), ENUM_REGISTRY_SETTINGS, &mut dm)
                };
                if ok == 0 {
                    continue;
                }
            }

            let ret = unsafe {
                ffi::ChangeDisplaySettingsExW(
                    name_wide.as_ptr(),
                    &mut dm,
                    std::ptr::null_mut(),
                    CDS_UPDATEREGISTRY | CDS_NORESET,
                    std::ptr::null_mut(),
                )
            };
            if ret == 0 {
                staged = true;
            }
        }
        if staged {
            unsafe {
                ffi::ChangeDisplaySettingsExW(
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                );
            }
            // Clear saved state after successful restore
            saved_modes().lock().unwrap().clear();
        }
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
