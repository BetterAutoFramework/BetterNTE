//! Win32 helper utilities for capture engines.
//!
//! Contains OS version detection and registry operations.

use std::env;
use tracing::info;

const DIRECT_X_KEY_PATH: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";
const DIRECT_X_VALUE_NAME: &str = "DirectXUserGlobalSettings";
const DIRECT_X_VALUE_DATA: &str = "SwapEffectUpgradeEnable=0;";

#[repr(C)]
struct RTL_OSVERSIONINFOEXW {
    dw_os_version_info_size: u32,
    dw_major_version: u32,
    dw_minor_version: u32,
    dw_build_number: u32,
    dw_platform_id: u32,
    sz_csd_version: [u16; 128],
    w_service_pack_major: u16,
    w_service_pack_minor: u16,
    w_suite_mask: u16,
    w_product_type: u8,
    w_reserved: u8,
}

pub fn is_windows11_or_greater() -> bool {
    if let Ok(v) = env::var("BetterNTE_Test_Win11") {
        return v == "1";
    }

    windows_build_number().is_some_and(|build| build >= 22000)
}

pub fn is_windows10_1903_or_greater() -> bool {
    windows_build_number().is_some_and(|build| build >= 18362)
}

fn windows_build_number() -> Option<u32> {
    if let Ok(v) = env::var("BetterNTE_Test_WinBuild") {
        if let Ok(parsed) = v.parse::<u32>() {
            return Some(parsed);
        }
    }

    #[cfg(windows)]
    {
        #[link(name = "ntdll")]
        extern "system" {
            fn RtlGetVersion(version_info: *mut RTL_OSVERSIONINFOEXW) -> i32;
        }

        unsafe {
            let mut version_info = RTL_OSVERSIONINFOEXW {
                dw_os_version_info_size: std::mem::size_of::<RTL_OSVERSIONINFOEXW>() as u32,
                dw_major_version: 0,
                dw_minor_version: 0,
                dw_build_number: 0,
                dw_platform_id: 0,
                sz_csd_version: [0; 128],
                w_service_pack_major: 0,
                w_service_pack_minor: 0,
                w_suite_mask: 0,
                w_product_type: 0,
                w_reserved: 0,
            };

            let ret = RtlGetVersion(&mut version_info);
            if ret == 0 {
                if version_info.dw_major_version >= 10 {
                    return Some(version_info.dw_build_number);
                }
                return None;
            }
        }
    }

    None
}

pub fn disable_win11_window_optimization() -> Result<bool, String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    match hkcu.open_subkey(DIRECT_X_KEY_PATH) {
        Ok(key) => {
            if let Ok(value) = key.get_value::<String, _>(DIRECT_X_VALUE_NAME) {
                if value == DIRECT_X_VALUE_DATA {
                    info!("Win11 window optimization already disabled");
                    return Ok(true);
                }
            }
            drop(key);
            info!("Updating DirectXUserGlobalSettings registry key");
        }
        Err(_) => {
            info!("Creating DirectXUserGlobalSettings registry key");
        }
    }

    let (key, _) = hkcu
        .create_subkey(DIRECT_X_KEY_PATH)
        .map_err(|e| format!("Failed to create registry key: {}", e))?;

    key.set_value(DIRECT_X_VALUE_NAME, &DIRECT_X_VALUE_DATA)
        .map_err(|e| format!("Failed to set registry value: {}", e))?;

    info!(
        "Win11 window optimization disabled: {}={}",
        DIRECT_X_VALUE_NAME, DIRECT_X_VALUE_DATA
    );
    info!("Registry change will take effect on next game start");

    Ok(true)
}

pub fn auto_fix_win11_bitblt() -> Result<bool, String> {
    if is_windows11_or_greater() {
        info!("Windows 11 detected, applying BitBlt compatibility fix");
        disable_win11_window_optimization()
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_windows11() {
        let result = is_windows11_or_greater();
        println!("Is Windows 11 or greater: {}", result);
    }
}
