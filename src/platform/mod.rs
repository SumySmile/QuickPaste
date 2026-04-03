#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::{
    HotkeyManager, HotkeySpec, TrayIconManager, choose_open_path, choose_save_path, confirm,
    current_cursor_position, parse_hotkey, set_launch_at_startup,
};

#[cfg(not(target_os = "windows"))]
mod stubs {
    use std::path::PathBuf;

    use crate::error::AppError;

    #[derive(Clone, Debug)]
    pub struct HotkeySpec {
        pub display: String,
    }

    pub struct HotkeyManager;
    pub struct TrayIconManager;

    impl HotkeyManager {
        pub fn start(
            _spec: HotkeySpec,
            _on_hotkey: impl Fn() + Send + 'static,
        ) -> Result<Self, AppError> {
            Err(AppError::UnsupportedPlatform(
                "Global hotkeys are currently supported on Windows only.",
            ))
        }

        pub fn update(&self, _spec: HotkeySpec) -> Result<(), AppError> {
            Err(AppError::UnsupportedPlatform(
                "Global hotkeys are currently supported on Windows only.",
            ))
        }
    }

    impl TrayIconManager {
        pub fn start(
            _on_activate: impl Fn() + Send + 'static,
            _on_settings: impl Fn() + Send + 'static,
            _on_exit: impl Fn() + Send + 'static,
        ) -> Result<Self, AppError> {
            Err(AppError::UnsupportedPlatform(
                "Tray icon is currently supported on Windows only.",
            ))
        }
    }

    pub fn parse_hotkey(value: &str) -> Result<HotkeySpec, AppError> {
        Ok(HotkeySpec {
            display: value.to_string(),
        })
    }

    pub fn choose_open_path() -> Option<PathBuf> {
        None
    }

    pub fn choose_save_path() -> Option<PathBuf> {
        None
    }

    pub fn confirm(_message: &str) -> bool {
        false
    }

    pub fn current_cursor_position() -> Result<(i32, i32), AppError> {
        Err(AppError::UnsupportedPlatform(
            "Window dragging is currently supported on Windows only.",
        ))
    }

    pub fn set_launch_at_startup(_enabled: bool) -> Result<(), AppError> {
        Err(AppError::UnsupportedPlatform(
            "Launch at startup is currently supported on Windows only.",
        ))
    }
}

#[cfg(not(target_os = "windows"))]
pub use stubs::{
    HotkeyManager, HotkeySpec, TrayIconManager, choose_open_path, choose_save_path, confirm,
    current_cursor_position, parse_hotkey, set_launch_at_startup,
};
