//! betternte-input/src/factory.rs
//! Input controller factory

use std::collections::HashMap;

use crate::adb::AdbInput;
use crate::controller::InputController;
use crate::mapper::KeyMapper;
use crate::target::InputTarget;

/// Create an input controller based on target type.
pub fn create_input_controller(
    target: &InputTarget,
    key_bindings: &HashMap<String, String>,
) -> Box<dyn InputController> {
    let mapper = KeyMapper::new(key_bindings.clone());

    match target {
        InputTarget::NativeWindow { .. } | InputTarget::NativeWindowBackground { .. } => {
            create_native_controller(mapper)
        }
        InputTarget::AdbDevice { serial } => Box::new(AdbInput::new(serial.clone(), mapper)),
        InputTarget::MumuEmulator { .. } => {
            // MuMu uses ADB input (memory direct reading possible in future)
            Box::new(AdbInput::new(String::new(), mapper))
        }
        InputTarget::LdEmulator { .. } => {
            // LD uses ADB input
            Box::new(AdbInput::new(String::new(), mapper))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_bindings() -> HashMap<String, String> {
        HashMap::new()
    }

    #[cfg(windows)]
    const NATIVE_NAME: &str = "Win32";
    #[cfg(target_os = "linux")]
    const NATIVE_NAME: &str = "Linux";
    #[cfg(target_os = "macos")]
    const NATIVE_NAME: &str = "macOS";

    #[test]
    fn test_native_window_creates_win32() {
        let target = InputTarget::NativeWindow { hwnd: 12345 };
        let ctrl = create_input_controller(&target, &empty_bindings());
        assert_eq!(ctrl.name(), NATIVE_NAME);
    }

    #[test]
    fn test_native_window_background_creates_win32() {
        let target = InputTarget::NativeWindowBackground { hwnd: 12345 };
        let ctrl = create_input_controller(&target, &empty_bindings());
        assert_eq!(ctrl.name(), NATIVE_NAME);
    }

    #[test]
    fn test_adb_device_creates_adb() {
        let target = InputTarget::AdbDevice {
            serial: "emulator-5554".into(),
        };
        let ctrl = create_input_controller(&target, &empty_bindings());
        assert_eq!(ctrl.name(), "ADB");
    }

    #[test]
    fn test_mumu_emulator_creates_adb() {
        let target = InputTarget::MumuEmulator { index: 0 };
        let ctrl = create_input_controller(&target, &empty_bindings());
        assert_eq!(ctrl.name(), "ADB");
    }

    #[test]
    fn test_ld_emulator_creates_adb() {
        let target = InputTarget::LdEmulator { index: 1 };
        let ctrl = create_input_controller(&target, &empty_bindings());
        assert_eq!(ctrl.name(), "ADB");
    }

    #[test]
    fn test_factory_with_key_bindings() {
        let mut bindings = HashMap::new();
        bindings.insert("enter".into(), "return".into());
        let target = InputTarget::NativeWindow { hwnd: 0 };
        let ctrl = create_input_controller(&target, &bindings);
        assert_eq!(ctrl.name(), NATIVE_NAME);
    }
}

/// Create a platform-specific native window input controller.
#[cfg(windows)]
fn create_native_controller(mapper: KeyMapper) -> Box<dyn InputController> {
    Box::new(crate::win32::Win32Input::new(mapper))
}

#[cfg(target_os = "linux")]
fn create_native_controller(mapper: KeyMapper) -> Box<dyn InputController> {
    Box::new(crate::linux::LinuxInput::new(mapper))
}

#[cfg(target_os = "macos")]
fn create_native_controller(mapper: KeyMapper) -> Box<dyn InputController> {
    Box::new(crate::macos::MacInput::new(mapper))
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn create_native_controller(_mapper: KeyMapper) -> Box<dyn InputController> {
    panic!("No native input backend for this platform. Use ADB for emulator input.")
}
