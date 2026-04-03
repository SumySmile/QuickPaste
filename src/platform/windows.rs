use std::ffi::OsStr;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST,
    OPENFILENAMEW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, RegisterHotKey, UnregisterHotKey, VK_F1,
};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CREATESTRUCTW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos, GetMessageW, GetWindowLongPtrW,
    IDI_APPLICATION, LoadIconW, MB_ICONQUESTION, MB_OKCANCEL, MF_STRING, MSG, MessageBoxW,
    PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostMessageW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, SetWindowLongPtrW, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON,
    TrackPopupMenu, TranslateMessage, WM_APP, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY,
    WM_HOTKEY, WM_LBUTTONUP, WM_NCCREATE, WM_NCDESTROY, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::{PCWSTR, PWSTR, w};

use crate::error::AppError;

const HOTKEY_ID: i32 = 1;
const TRAY_ICON_ID: u32 = 1;
const TRAY_CALLBACK_MESSAGE: u32 = WM_APP + 1;
const TRAY_MENU_SETTINGS_ID: usize = 1001;
const TRAY_MENU_CLOSE_ID: usize = 1002;
const TRAY_CLASS_NAME: &str = "MyQuickPasteTrayWindow";
const TRAY_TOOLTIP: &str = "Quick Paste";
const RUN_KEY_PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE_NAME: &str = "MyQuickPaste";

#[derive(Clone, Debug)]
pub struct HotkeySpec {
    pub modifiers: HOT_KEY_MODIFIERS,
    pub virtual_key: u32,
    pub display: String,
}

enum HotkeyCommand {
    Update(HotkeySpec, Sender<Result<(), AppError>>),
    Stop,
}

pub struct HotkeyManager {
    sender: Sender<HotkeyCommand>,
}

pub struct TrayIconManager {
    hwnd: HWND,
}

type TrayCallback = Box<dyn Fn() + Send + 'static>;

struct TrayWindowState {
    on_activate: TrayCallback,
    on_settings: TrayCallback,
    on_exit: TrayCallback,
}

impl HotkeyManager {
    pub fn start(
        spec: HotkeySpec,
        on_hotkey: impl Fn() + Send + 'static,
    ) -> Result<Self, AppError> {
        let (sender, receiver) = mpsc::channel::<HotkeyCommand>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), AppError>>();

        thread::spawn(move || {
            let mut message = MSG::default();
            unsafe {
                let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
            }

            let mut current = spec;
            if let Err(err) = register_hotkey(&current) {
                let _ = ready_tx.send(Err(AppError::validation(err.to_string())));
                return;
            }
            let _ = ready_tx.send(Ok(()));

            loop {
                unsafe {
                    while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).into() {
                        if message.message == WM_HOTKEY {
                            on_hotkey();
                        }
                    }
                }

                match receiver.recv_timeout(Duration::from_millis(50)) {
                    Ok(HotkeyCommand::Update(next, response_tx)) => {
                        unsafe {
                            unregister_hotkey();
                        }
                        match register_hotkey(&next) {
                            Ok(()) => {
                                current = next;
                                let _ = response_tx.send(Ok(()));
                            }
                            Err(err) => {
                                let _ = register_hotkey(&current);
                                let _ = response_tx.send(Err(err));
                            }
                        }
                    }
                    Ok(HotkeyCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }

            unsafe {
                unregister_hotkey();
            }
        });

        ready_rx.recv().unwrap_or_else(|_| {
            Err(AppError::validation(
                "Hotkey listener failed to initialize.",
            ))
        })?;

        Ok(Self { sender })
    }

    pub fn update(&self, spec: HotkeySpec) -> Result<(), AppError> {
        let (response_tx, response_rx) = mpsc::channel();
        self.sender
            .send(HotkeyCommand::Update(spec, response_tx))
            .map_err(|_| AppError::validation("Hotkey listener is no longer running."))?;

        response_rx
            .recv()
            .unwrap_or_else(|_| Err(AppError::validation("Hotkey update failed.")))
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        let _ = self.sender.send(HotkeyCommand::Stop);
    }
}

impl TrayIconManager {
    pub fn start(
        on_activate: impl Fn() + Send + 'static,
        on_settings: impl Fn() + Send + 'static,
        on_exit: impl Fn() + Send + 'static,
    ) -> Result<Self, AppError> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<isize, AppError>>();

        thread::spawn(move || {
            let state = Box::new(TrayWindowState {
                on_activate: Box::new(on_activate),
                on_settings: Box::new(on_settings),
                on_exit: Box::new(on_exit),
            });

            let hwnd = match create_tray_window(state) {
                Ok(hwnd) => hwnd,
                Err(err) => {
                    let _ = ready_tx.send(Err(err));
                    return;
                }
            };

            let _ = ready_tx.send(Ok(hwnd.0 as isize));

            let mut message = MSG::default();
            loop {
                let status = unsafe { GetMessageW(&mut message, None, 0, 0) };
                if status.0 <= 0 {
                    break;
                }

                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        });

        let hwnd = ready_rx
            .recv()
            .unwrap_or_else(|_| Err(AppError::validation("Tray icon failed to initialize.")))?;

        Ok(Self {
            hwnd: HWND(hwnd as *mut _),
        })
    }
}

impl Drop for TrayIconManager {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn parse_hotkey(value: &str) -> Result<HotkeySpec, AppError> {
    let mut modifiers = HOT_KEY_MODIFIERS(0);
    let mut key_name = None;

    for part in value
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part.to_ascii_lowercase().as_str() {
            "alt" => modifiers |= MOD_ALT,
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "shift" => modifiers |= MOD_SHIFT,
            other => key_name = Some(other.to_string()),
        }
    }

    let key_name = key_name.ok_or_else(|| AppError::validation("Hotkey needs a primary key."))?;

    if modifiers.0 == 0 {
        return Err(AppError::validation(
            "Hotkey needs at least one modifier key.",
        ));
    }

    let virtual_key = parse_virtual_key(&key_name)
        .ok_or_else(|| AppError::validation(format!("Unsupported hotkey key: {key_name}")))?;

    Ok(HotkeySpec {
        modifiers,
        virtual_key,
        display: format_hotkey(modifiers, virtual_key),
    })
}

pub fn choose_open_path() -> Option<PathBuf> {
    unsafe {
        let mut file_buffer = [0u16; 260];
        let filter = wide("TOML Files (*.toml)\0*.toml\0All Files (*.*)\0*.*\0\0");
        let mut dialog = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: HWND::default(),
            lpstrFile: PWSTR(file_buffer.as_mut_ptr()),
            nMaxFile: file_buffer.len() as u32,
            lpstrFilter: PCWSTR(filter.as_ptr()),
            Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
            ..Default::default()
        };

        if GetOpenFileNameW(&mut dialog).as_bool() {
            Some(PathBuf::from(wide_slice_to_string(&file_buffer)))
        } else {
            None
        }
    }
}

pub fn choose_save_path() -> Option<PathBuf> {
    unsafe {
        let mut file_buffer = wide("quick-paste.toml");
        file_buffer.resize(260, 0);
        let filter = wide("TOML Files (*.toml)\0*.toml\0All Files (*.*)\0*.*\0\0");
        let mut dialog = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: HWND::default(),
            lpstrFile: PWSTR(file_buffer.as_mut_ptr()),
            nMaxFile: file_buffer.len() as u32,
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrDefExt: w!("toml"),
            Flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST,
            ..Default::default()
        };

        if GetSaveFileNameW(&mut dialog).as_bool() {
            Some(PathBuf::from(wide_slice_to_string(&file_buffer)))
        } else {
            None
        }
    }
}

pub fn confirm(message: &str) -> bool {
    unsafe {
        let text = wide(message);
        MessageBoxW(
            Some(HWND::default()),
            PCWSTR(text.as_ptr()),
            w!("Quick Paste"),
            MB_OKCANCEL | MB_ICONQUESTION,
        )
        .0 == 1
    }
}

pub fn current_cursor_position() -> Result<(i32, i32), AppError> {
    let mut point = POINT::default();
    unsafe {
        GetCursorPos(&mut point).map_err(|err| {
            AppError::validation(format!("Failed to read cursor position: {err}"))
        })?;
    }
    Ok((point.x, point.y))
}

pub fn set_launch_at_startup(enabled: bool) -> Result<(), AppError> {
    if enabled {
        set_run_value(&quoted_current_exe()?)
    } else {
        remove_run_value()
    }
}

fn create_tray_window(state: Box<TrayWindowState>) -> Result<HWND, AppError> {
    let module = unsafe {
        GetModuleHandleW(None)
            .map_err(|err| AppError::validation(format!("Failed to load module handle: {err}")))?
    };
    let instance = HINSTANCE(module.0);
    let icon = load_app_icon(instance)?;
    let class_name = wide(TRAY_CLASS_NAME);

    let window_class = WNDCLASSW {
        lpfnWndProc: Some(tray_window_proc),
        hInstance: instance,
        hIcon: icon,
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };

    let class_atom = unsafe { RegisterClassW(&window_class) };
    if class_atom == 0 {
        return Err(AppError::validation(
            "Failed to register tray window class.",
        ));
    }

    let state_ptr = Box::into_raw(state);
    let hwnd = unsafe {
        CreateWindowExW(
            Default::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(class_name.as_ptr()),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            Some(state_ptr.cast()),
        )
    };

    let hwnd = match hwnd {
        Ok(hwnd) => hwnd,
        Err(err) => {
            unsafe {
                drop(Box::from_raw(state_ptr));
            }
            return Err(AppError::validation(format!(
                "Failed to create tray window: {err}"
            )));
        }
    };

    add_tray_icon(hwnd, icon)?;
    Ok(hwnd)
}

fn load_app_icon(
    instance: HINSTANCE,
) -> Result<windows::Win32::UI::WindowsAndMessaging::HICON, AppError> {
    unsafe {
        LoadIconW(Some(instance), PCWSTR(1 as *const u16))
            .or_else(|_| LoadIconW(None, IDI_APPLICATION))
            .map_err(|err| AppError::validation(format!("Failed to load app icon: {err}")))
    }
}
fn add_tray_icon(
    hwnd: HWND,
    icon: windows::Win32::UI::WindowsAndMessaging::HICON,
) -> Result<(), AppError> {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY_CALLBACK_MESSAGE,
        hIcon: icon,
        ..Default::default()
    };
    copy_wide(TRAY_TOOLTIP, &mut data.szTip);

    let added = unsafe { Shell_NotifyIconW(NIM_ADD, &data) };
    if !added.as_bool() {
        return Err(AppError::validation("Failed to add tray icon."));
    }

    Ok(())
}

fn remove_tray_icon(hwnd: HWND) {
    let data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_ID,
        ..Default::default()
    };

    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &data);
    }
}

unsafe extern "system" fn tray_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_NCCREATE => {
            let create_struct = lparam.0 as *const CREATESTRUCTW;
            if create_struct.is_null() {
                return LRESULT(0);
            }

            let state_ptr = unsafe { (*create_struct).lpCreateParams as *mut TrayWindowState };
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
            }

            LRESULT(1)
        }
        WM_CREATE => LRESULT(0),
        WM_COMMAND => {
            let Some(state) = tray_state(hwnd) else {
                return LRESULT(0);
            };
            let command_id = wparam.0 & 0xffff;

            match command_id {
                TRAY_MENU_SETTINGS_ID => (state.on_settings)(),
                TRAY_MENU_CLOSE_ID => {
                    (state.on_exit)();
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
                _ => {}
            }

            LRESULT(0)
        }
        TRAY_CALLBACK_MESSAGE => {
            let Some(state) = tray_state(hwnd) else {
                return LRESULT(0);
            };

            match lparam.0 as u32 {
                WM_LBUTTONUP => {
                    (state.on_activate)();
                }
                WM_RBUTTONUP => {
                    let _ = show_tray_menu(hwnd);
                }
                _ => {}
            }

            LRESULT(0)
        }
        WM_DESTROY => {
            remove_tray_icon(hwnd);
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let state_ptr =
                unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayWindowState };
            if !state_ptr.is_null() {
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    drop(Box::from_raw(state_ptr));
                }
            }

            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn tray_state(hwnd: HWND) -> Option<&'static TrayWindowState> {
    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayWindowState };
    if state_ptr.is_null() {
        None
    } else {
        Some(unsafe { &*state_ptr })
    }
}

fn show_tray_menu(hwnd: HWND) -> Result<(), AppError> {
    let menu = unsafe {
        CreatePopupMenu()
            .map_err(|err| AppError::validation(format!("Failed to create tray menu: {err}")))?
    };

    let settings = wide("Settings");
    let close = wide("Close");

    unsafe {
        AppendMenuW(
            menu,
            MF_STRING,
            TRAY_MENU_SETTINGS_ID,
            PCWSTR(settings.as_ptr()),
        )
        .map_err(|err| AppError::validation(format!("Failed to build tray menu: {err}")))?;
        AppendMenuW(menu, MF_STRING, TRAY_MENU_CLOSE_ID, PCWSTR(close.as_ptr()))
            .map_err(|err| AppError::validation(format!("Failed to build tray menu: {err}")))?;
    }

    let mut point = POINT::default();
    unsafe {
        GetCursorPos(&mut point)
            .map_err(|err| AppError::validation(format!("Failed to position tray menu: {err}")))?;
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            None,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
    }

    Ok(())
}

fn register_hotkey(spec: &HotkeySpec) -> Result<(), AppError> {
    unsafe {
        RegisterHotKey(None, HOTKEY_ID, spec.modifiers, spec.virtual_key).map_err(|_| {
            AppError::validation(format!("Unable to register hotkey {}.", spec.display))
        })?;
    }
    Ok(())
}

unsafe fn unregister_hotkey() {
    unsafe {
        let _ = UnregisterHotKey(None, HOTKEY_ID);
    }
}

fn set_run_value(value: &str) -> Result<(), AppError> {
    let subkey = wide(RUN_KEY_PATH);
    let value_name = wide(RUN_VALUE_NAME);
    let mut key = HKEY::default();

    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    };

    if status != ERROR_SUCCESS {
        return Err(AppError::validation(format!(
            "Failed to update startup setting (code {}).",
            status.0
        )));
    }

    let wide_value = wide(value);
    let byte_len = wide_value.len() * size_of::<u16>();
    let bytes = unsafe { std::slice::from_raw_parts(wide_value.as_ptr() as *const u8, byte_len) };
    let set_status =
        unsafe { RegSetValueExW(key, PCWSTR(value_name.as_ptr()), None, REG_SZ, Some(bytes)) };
    unsafe {
        let _ = RegCloseKey(key);
    }

    if set_status != ERROR_SUCCESS {
        return Err(AppError::validation(format!(
            "Failed to enable startup (code {}).",
            set_status.0
        )));
    }

    Ok(())
}

fn remove_run_value() -> Result<(), AppError> {
    let subkey = wide(RUN_KEY_PATH);
    let value_name = wide(RUN_VALUE_NAME);
    let mut key = HKEY::default();

    let open_status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            KEY_SET_VALUE,
            &mut key,
        )
    };

    if open_status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }

    if open_status != ERROR_SUCCESS {
        return Err(AppError::validation(format!(
            "Failed to update startup setting (code {}).",
            open_status.0
        )));
    }

    let delete_status = unsafe { RegDeleteValueW(key, PCWSTR(value_name.as_ptr())) };
    unsafe {
        let _ = RegCloseKey(key);
    }

    if delete_status == ERROR_SUCCESS || delete_status == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(AppError::validation(format!(
            "Failed to disable startup (code {}).",
            delete_status.0
        )))
    }
}

fn quoted_current_exe() -> Result<String, AppError> {
    let path = std::env::current_exe()?;
    Ok(format!("\"{}\"", path.to_string_lossy()))
}

fn format_hotkey(modifiers: HOT_KEY_MODIFIERS, virtual_key: u32) -> String {
    let mut parts = Vec::new();
    if modifiers.contains(MOD_CONTROL) {
        parts.push("Ctrl".to_string());
    }
    if modifiers.contains(MOD_ALT) {
        parts.push("Alt".to_string());
    }
    if modifiers.contains(MOD_SHIFT) {
        parts.push("Shift".to_string());
    }

    parts.push(match virtual_key {
        value if value >= VK_F1.0 as u32 && value <= VK_F1.0 as u32 + 11 => {
            format!("F{}", value - VK_F1.0 as u32 + 1)
        }
        value => char::from_u32(value)
            .map(|ch| ch.to_string())
            .unwrap_or_else(|| format!("0x{value:X}")),
    });

    parts.join("+")
}

fn parse_virtual_key(value: &str) -> Option<u32> {
    let upper = value.to_ascii_uppercase();
    if upper.len() == 1 {
        let ch = upper.chars().next()?;
        if ch.is_ascii_alphanumeric() {
            return Some(ch as u32);
        }
    }

    if let Some(number) = upper.strip_prefix('F') {
        let function = number.parse::<u32>().ok()?;
        if (1..=12).contains(&function) {
            return Some(VK_F1.0 as u32 + function - 1);
        }
    }

    None
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

fn wide_slice_to_string(value: &[u16]) -> String {
    let length = value.iter().position(|ch| *ch == 0).unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}

fn copy_wide(value: &str, destination: &mut [u16]) {
    if destination.is_empty() {
        return;
    }

    let encoded = wide(value);
    let copy_len = encoded.len().min(destination.len());
    destination[..copy_len].copy_from_slice(&encoded[..copy_len]);

    if copy_len == destination.len() {
        destination[destination.len() - 1] = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::parse_hotkey;

    #[test]
    fn parse_hotkey_accepts_alt_v() {
        let spec = parse_hotkey("Alt+V").expect("Alt+V should be valid");
        assert_eq!(spec.display, "Alt+V");
    }

    #[test]
    fn parse_hotkey_rejects_missing_modifier() {
        assert!(parse_hotkey("V").is_err());
    }
}


