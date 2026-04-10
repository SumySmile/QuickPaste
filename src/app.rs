use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use arboard::Clipboard;
use slint::{
    CloseRequestResponse, ComponentHandle, LogicalSize, ModelRc, PhysicalPosition, Timer, VecModel,
    Weak,
};

use crate::config::{AppConfig, MAX_ITEMS_PER_TAB, MAX_TABS};
use crate::error::AppError;
use crate::paths::AppPaths;
use crate::platform::{
    HotkeyManager, SingleInstance, SingleInstanceState, TrayIconManager, apply_app_icon_to_window,
    bring_window_to_front, choose_open_path, choose_save_path, confirm, current_cursor_position,
    current_monitor_scale_factor, current_monitor_work_area, current_window_rect,
    current_window_scale_factor, current_window_work_area, parse_hotkey, set_launch_at_startup,
};
use crate::{AppWindow, ItemViewData, OverlayKind, SettingsWindow, TabViewData};

const TAB_DROP_SLOT_WIDTH: f32 = 80.0;
const ITEM_DROP_SLOT_HEIGHT: f32 = 80.0;
const HOME_WINDOW_TITLE: &str = "Quick Paste";
const SETTINGS_WINDOW_TITLE: &str = "Quick Paste Settings";
const HOME_WINDOW_WIDTH: i32 = 440;
const HOME_WINDOW_HEIGHT: i32 = 664;
const SETTINGS_WINDOW_WIDTH: i32 = 368;
const SETTINGS_WINDOW_HEIGHT: i32 = 456;
const WINDOW_SAFE_MARGIN: i32 = 16;

pub fn run() -> Result<(), AppError> {
    let paths = AppPaths::discover()?;
    let config = AppConfig::load_or_default(&paths.config_path)?;

    let window = AppWindow::new()
        .map_err(|err| AppError::validation(format!("Failed to create the Slint window: {err}")))?;
    let settings_window = SettingsWindow::new().map_err(|err| {
        AppError::validation(format!("Failed to create the Slint settings window: {err}"))
    })?;

    window
        .window()
        .on_close_requested(|| CloseRequestResponse::HideWindow);
    settings_window
        .window()
        .on_close_requested(|| CloseRequestResponse::HideWindow);

    let activation_weak = window.as_weak();
    let activation_settings_weak = settings_window.as_weak();
    let _single_instance = match SingleInstance::start({
        let weak = activation_weak.clone();
        let settings_weak = activation_settings_weak.clone();
        move || show_home_from_tray(&weak, &settings_weak)
    })? {
        SingleInstanceState::Primary(manager) => manager,
        SingleInstanceState::Secondary => return Ok(()),
    };

    let hotkey = parse_hotkey(&config.hotkey)?;
    let hotkey_weak = window.as_weak();
    let settings_weak = settings_window.as_weak();
    let hotkey_manager = HotkeyManager::start(hotkey, move || {
        toggle_window_visibility(&hotkey_weak, &settings_weak)
    })?;

    let controller = Rc::new(RefCell::new(AppController::new(
        paths,
        config,
        hotkey_manager,
    )));
    controller.borrow().sync_window(&window);
    controller.borrow().sync_settings_window(&settings_window);
    wire_callbacks(&window, &settings_window, &controller);
    wire_settings_callbacks(&settings_window, &window, &controller);

    let tray_weak = window.as_weak();
    let tray_settings_weak = settings_window.as_weak();
    let _tray_manager = TrayIconManager::start(
        {
            let weak = tray_weak.clone();
            let settings_weak = tray_settings_weak.clone();
            move || show_home_from_tray(&weak, &settings_weak)
        },
        {
            let weak = tray_weak.clone();
            let settings_weak = tray_settings_weak.clone();
            move || show_settings_from_tray(&weak, &settings_weak)
        },
        move || quit_from_tray(),
    )?;

    {
        let startup_window = window.as_weak();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = startup_window.upgrade() {
                window.invoke_open_home();
                let _ = show_component_window(
                    &window,
                    HOME_WINDOW_TITLE,
                    (HOME_WINDOW_WIDTH, HOME_WINDOW_HEIGHT),
                );
            }
        });
    }

    slint::run_event_loop_until_quit()
        .map_err(|err| AppError::validation(format!("Failed to run the Slint event loop: {err}")))
}

struct AppController {
    paths: AppPaths,
    config: AppConfig,
    hotkey_manager: HotkeyManager,
    selected_tab: Option<usize>,
    overlay: OverlayKind,
    delete_mode: bool,
    pending_delete_item: Option<usize>,
    draft_hotkey: String,
    hotkey_recording: bool,
    draft_tab_name: String,
    draft_item_title: String,
    draft_item_content: String,
    editing_tab_index: Option<usize>,
    editing_item_index: Option<usize>,
    drag_origin_cursor: Option<(i32, i32)>,
    drag_origin_window: Option<PhysicalPosition>,
    status_text: String,
    status_is_error: bool,
    status_revision: u64,
    copied_toast_text: String,
    copied_toast_revision: u64,
}

impl AppController {
    fn new(paths: AppPaths, config: AppConfig, hotkey_manager: HotkeyManager) -> Self {
        let mut controller = Self {
            draft_hotkey: config.hotkey.clone(),
            hotkey_recording: false,
            paths,
            selected_tab: if config.tabs.is_empty() {
                None
            } else {
                Some(0)
            },
            config,
            hotkey_manager,
            overlay: OverlayKind::None,
            delete_mode: false,
            pending_delete_item: None,
            draft_tab_name: String::new(),
            draft_item_title: String::new(),
            draft_item_content: String::new(),
            editing_tab_index: None,
            editing_item_index: None,
            drag_origin_cursor: None,
            drag_origin_window: None,
            status_text: String::new(),
            status_is_error: false,
            status_revision: 0,
            copied_toast_text: String::new(),
            copied_toast_revision: 0,
        };

        if let Err(err) = set_launch_at_startup(controller.config.launch_at_startup) {
            controller.set_status(err.to_string(), true);
        }

        controller
    }

    fn sync_window(&self, window: &AppWindow) {
        window.set_overlay(self.overlay);
        window.set_delete_mode(self.delete_mode);
        window.set_has_tabs(!self.config.tabs.is_empty());
        window.set_has_current_tab(self.selected_tab.is_some());
        window.set_status_text(self.status_text.clone().into());
        window.set_status_is_error(self.status_is_error);
        window.set_copied_toast_text(self.copied_toast_text.clone().into());
        window.set_show_copied_toast(!self.copied_toast_text.is_empty());
        window.set_draft_tab_name(self.draft_tab_name.clone().into());
        window.set_draft_item_title(self.draft_item_title.clone().into());
        window.set_draft_item_content(self.draft_item_content.clone().into());
        window.set_can_add_tab(self.config.tabs.len() < MAX_TABS);

        let items = self.current_items();
        let can_add_item = self
            .selected_tab
            .and_then(|index| self.config.tabs.get(index))
            .is_some_and(|tab| tab.items.len() < MAX_ITEMS_PER_TAB);

        window.set_can_add_item(can_add_item);
        window.set_show_delete_toggle(self.selected_tab.is_some() && !items.is_empty());
        window.set_show_empty_state(items.is_empty());

        let (empty_title, empty_hint) = self.empty_state_text();
        window.set_empty_title(empty_title.into());
        window.set_empty_hint(empty_hint.into());
        window.set_tabs(ModelRc::from(Rc::new(VecModel::from(self.tab_rows()))));
        window.set_items(ModelRc::from(Rc::new(VecModel::from(self.item_rows()))));
    }

    fn sync_settings_window(&self, window: &SettingsWindow) {
        window.set_status_text(self.status_text.clone().into());
        window.set_status_is_error(self.status_is_error);
        window.set_hotkey_recording(self.hotkey_recording);
        window.set_launch_at_startup(self.config.launch_at_startup);
        window.set_draft_hotkey(self.draft_hotkey.clone().into());
    }

    fn tab_rows(&self) -> Vec<TabViewData> {
        self.config
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| TabViewData {
                title: tab.name.clone().into(),
                selected: Some(index) == self.selected_tab,
            })
            .collect()
    }

    fn item_rows(&self) -> Vec<ItemViewData> {
        self.current_items()
            .iter()
            .enumerate()
            .map(|(index, item)| ItemViewData {
                title: item.title.clone().into(),
                preview: preview_text(&item.content).into(),
                content: item.content.clone().into(),
                can_delete: self.delete_mode,
                editing: Some(index) == self.editing_item_index,
            })
            .collect()
    }

    fn current_items(&self) -> &[crate::config::ItemConfig] {
        self.selected_tab
            .and_then(|index| self.config.tabs.get(index))
            .map(|tab| tab.items.as_slice())
            .unwrap_or(&[])
    }

    fn empty_state_text(&self) -> (&'static str, &'static str) {
        if self.config.tabs.is_empty() {
            ("No tabs yet", "Use the tab + to create your first group.")
        } else {
            ("No items yet", "Use the bottom-right + to add content.")
        }
    }

    fn set_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.status_text = message.into();
        self.status_is_error = is_error;
        self.status_revision = self.status_revision.wrapping_add(1);
    }

    fn clear_status(&mut self) {
        self.status_text.clear();
        self.status_is_error = false;
        self.status_revision = self.status_revision.wrapping_add(1);
    }

    fn set_copied_toast(&mut self, message: impl Into<String>) {
        self.copied_toast_text = message.into();
        self.copied_toast_revision = self.copied_toast_revision.wrapping_add(1);
    }

    fn clear_copied_toast(&mut self) {
        self.copied_toast_text.clear();
        self.copied_toast_revision = self.copied_toast_revision.wrapping_add(1);
    }

    fn persist_config(&mut self) {
        match self.config.save_to_path(&self.paths.config_path) {
            Ok(()) => {}
            Err(err) => self.set_status(err.to_string(), true),
        }
    }

    fn reset_overlay_state(&mut self) {
        self.overlay = OverlayKind::None;
        self.pending_delete_item = None;
        self.editing_tab_index = None;
        self.editing_item_index = None;
        self.hotkey_recording = false;
        self.draft_tab_name.clear();
        self.draft_item_title.clear();
        self.draft_item_content.clear();
    }

    fn select_tab(&mut self, index: usize) {
        if index < self.config.tabs.len() {
            self.selected_tab = Some(index);
            self.reset_overlay_state();
            self.delete_mode = false;
            self.clear_status();
        }
    }

    fn open_settings(&mut self) {
        self.draft_hotkey = self.config.hotkey.clone();
        self.hotkey_recording = false;
        self.clear_status();
    }

    fn open_home(&mut self) {
        self.reset_overlay_state();
        self.delete_mode = false;
        self.stop_window_drag();
        self.clear_status();
    }

    fn close_settings(&mut self) {
        self.hotkey_recording = false;
        self.clear_status();
    }

    fn open_add_tab(&mut self) {
        if self.config.tabs.len() >= MAX_TABS {
            self.set_status(format!("Up to {MAX_TABS} tabs."), true);
            return;
        }

        self.reset_overlay_state();
        self.overlay = OverlayKind::AddTab;
        self.clear_status();
    }

    fn open_rename_tab(&mut self, index: usize) {
        let Some(tab_name) = self.config.tabs.get(index).map(|tab| tab.name.clone()) else {
            return;
        };

        self.selected_tab = Some(index);
        self.reset_overlay_state();
        self.overlay = OverlayKind::RenameTab;
        self.editing_tab_index = Some(index);
        self.draft_tab_name = tab_name;
        self.clear_status();
    }

    fn submit_tab_form(&mut self, value: String) {
        match self.overlay {
            OverlayKind::AddTab => self.add_tab(value),
            OverlayKind::RenameTab => self.rename_tab(value),
            _ => {}
        }
    }

    fn add_tab(&mut self, name: String) {
        match self.config.add_tab(name.trim()) {
            Ok(index) => {
                self.selected_tab = Some(index);
                self.reset_overlay_state();
                self.delete_mode = false;
                self.persist_config();
                if !self.status_is_error {
                    self.set_status("Tab added.", false);
                }
            }
            Err(err) => self.set_status(err.to_string(), true),
        }
    }

    fn rename_tab(&mut self, name: String) {
        let Some(index) = self.editing_tab_index else {
            return;
        };

        let trimmed = name.trim();
        let Some(tab) = self.config.tabs.get_mut(index) else {
            return;
        };

        tab.name = trimmed.to_string();
        match self.config.validate() {
            Ok(()) => {
                self.selected_tab = Some(index);
                self.reset_overlay_state();
                self.persist_config();
                if !self.status_is_error {
                    self.set_status("Tab renamed.", false);
                }
            }
            Err(err) => self.set_status(err.to_string(), true),
        }
    }

    fn request_delete_tab(&mut self, index: usize) {
        if index >= self.config.tabs.len() {
            return;
        }

        self.selected_tab = Some(index);
        self.reset_overlay_state();
        self.overlay = OverlayKind::ConfirmDeleteTab;
        self.clear_status();
    }

    fn confirm_delete_tab(&mut self) {
        let Some(index) = self.selected_tab else {
            return;
        };

        match self.config.remove_tab(index) {
            Ok(()) => {
                self.selected_tab = clamp_selected_tab(index, self.config.tabs.len());
                self.reset_overlay_state();
                self.delete_mode = false;
                self.persist_config();
                if !self.status_is_error {
                    self.set_status("Tab deleted.", false);
                }
            }
            Err(err) => self.set_status(err.to_string(), true),
        }
    }

    fn open_add_item(&mut self) {
        let Some(index) = self.selected_tab else {
            return;
        };

        if self.config.tabs[index].items.len() >= MAX_ITEMS_PER_TAB {
            self.set_status(format!("Up to {MAX_ITEMS_PER_TAB} items per tab."), true);
            return;
        }

        self.reset_overlay_state();
        self.overlay = OverlayKind::AddItem;
        self.clear_status();
    }

    fn open_edit_item(&mut self, index: usize) {
        let Some(tab_index) = self.selected_tab else {
            return;
        };
        let Some(title) = self
            .config
            .tabs
            .get(tab_index)
            .and_then(|tab| tab.items.get(index))
            .map(|item| item.title.clone())
        else {
            return;
        };

        self.reset_overlay_state();
        self.editing_item_index = Some(index);
        self.draft_item_title = title;
        self.clear_status();
    }

    fn submit_item_form(&mut self, title: String, content: String) {
        match self.overlay {
            OverlayKind::AddItem => self.add_item(title, content),
            _ => {}
        }
    }

    fn add_item(&mut self, title: String, content: String) {
        let Some(tab_index) = self.selected_tab else {
            return;
        };

        match self
            .config
            .add_item(tab_index, title.trim(), content.trim())
        {
            Ok(()) => {
                self.reset_overlay_state();
                self.persist_config();
                if !self.status_is_error {
                    self.set_status("Item added.", false);
                }
            }
            Err(err) => self.set_status(err.to_string(), true),
        }
    }

    fn edit_item(&mut self, title: String, _content: String) {
        let Some(tab_index) = self.selected_tab else {
            return;
        };
        let Some(item_index) = self.editing_item_index else {
            return;
        };

        let Some(item) = self
            .config
            .tabs
            .get_mut(tab_index)
            .and_then(|tab| tab.items.get_mut(item_index))
        else {
            return;
        };

        item.title = title.trim().to_string();

        match self.config.validate() {
            Ok(()) => {
                self.reset_overlay_state();
                self.persist_config();
                if !self.status_is_error {
                    self.clear_status();
                }
            }
            Err(err) => self.set_status(err.to_string(), true),
        }
    }

    fn save_inline_item_edit(&mut self) {
        if self.editing_item_index.is_none() {
            return;
        }

        self.edit_item(self.draft_item_title.clone(), String::new());
    }

    fn cancel_inline_item_edit(&mut self) {
        if self.editing_item_index.is_none() {
            return;
        }

        self.editing_item_index = None;
        self.draft_item_title.clear();
        self.clear_status();
    }

    fn toggle_delete_mode(&mut self) {
        if self.current_items().is_empty() {
            return;
        }

        self.delete_mode = !self.delete_mode;
        self.reset_overlay_state();
        self.clear_status();
    }

    fn request_delete_item(&mut self, index: usize) {
        if !self.delete_mode || index >= self.current_items().len() {
            return;
        }

        self.reset_overlay_state();
        self.pending_delete_item = Some(index);
        self.overlay = OverlayKind::ConfirmDeleteItem;
        self.clear_status();
    }

    fn confirm_delete_item(&mut self) {
        let Some(tab_index) = self.selected_tab else {
            return;
        };
        let Some(item_index) = self.pending_delete_item.take() else {
            return;
        };

        match self.config.remove_item(tab_index, item_index) {
            Ok(()) => {
                self.reset_overlay_state();
                self.delete_mode = !self.current_items().is_empty();
                self.persist_config();
                if !self.status_is_error {
                    self.set_status("Item deleted.", false);
                }
            }
            Err(err) => self.set_status(err.to_string(), true),
        }
    }

    fn reorder_tab(&mut self, from_index: usize, position_x: f32) {
        if from_index >= self.config.tabs.len() {
            return;
        }

        let target_index =
            drop_target_index(position_x, TAB_DROP_SLOT_WIDTH, self.config.tabs.len());
        if target_index == from_index {
            return;
        }

        match self.config.move_tab(from_index, target_index) {
            Ok(()) => {
                self.selected_tab = Some(target_index);
                self.persist_config();
                if !self.status_is_error {
                    self.set_status("Tab moved.", false);
                }
            }
            Err(err) => self.set_status(err.to_string(), true),
        }
    }

    fn reorder_item(&mut self, from_index: usize, position_y: f32) {
        let Some(tab_index) = self.selected_tab else {
            return;
        };

        let item_count = self.current_items().len();
        if from_index >= item_count {
            return;
        }

        let target_index = drop_target_index(position_y, ITEM_DROP_SLOT_HEIGHT, item_count);
        if target_index == from_index {
            return;
        }

        match self.config.move_item(tab_index, from_index, target_index) {
            Ok(()) => {
                self.persist_config();
                if !self.status_is_error {
                    self.set_status("Item moved.", false);
                }
            }
            Err(err) => self.set_status(err.to_string(), true),
        }
    }

    fn cancel_overlay(&mut self) {
        self.reset_overlay_state();
        self.clear_status();
    }

    fn save_hotkey(&mut self, hotkey: String) {
        let parsed = match parse_hotkey(&hotkey) {
            Ok(parsed) => parsed,
            Err(err) => {
                self.set_status(err.to_string(), true);
                return;
            }
        };

        if let Err(err) = self.hotkey_manager.update(parsed.clone()) {
            self.set_status(err.to_string(), true);
            return;
        }

        self.config.hotkey = parsed.display.clone();
        self.draft_hotkey = parsed.display;
        self.persist_config();
        if !self.status_is_error {
            self.set_status("Hotkey saved.", false);
        }
    }

    fn begin_hotkey_recording(&mut self) {
        self.hotkey_recording = true;
        self.clear_status();
    }

    fn end_hotkey_recording(&mut self) {
        self.hotkey_recording = false;
    }

    fn capture_hotkey(&mut self, key_text: String, alt: bool, ctrl: bool, shift: bool) {
        let trimmed = key_text.trim();

        if trimmed == "\u{1b}" {
            self.hotkey_recording = false;
            self.clear_status();
            return;
        }

        if trimmed.is_empty() || is_modifier_key(trimmed) {
            return;
        }

        if !alt && !ctrl && !shift {
            return;
        }

        let Ok(candidate) = build_hotkey_candidate(trimmed, alt, ctrl, shift) else {
            return;
        };

        let Ok(parsed) = parse_hotkey(&candidate) else {
            return;
        };

        self.draft_hotkey = parsed.display;
        self.hotkey_recording = false;
        self.clear_status();
    }

    fn toggle_launch_at_startup(&mut self) {
        let next = !self.config.launch_at_startup;
        if let Err(err) = set_launch_at_startup(next) {
            self.set_status(err.to_string(), true);
            return;
        }

        self.config.launch_at_startup = next;
        self.persist_config();
        if !self.status_is_error {
            self.set_status(if next { "Startup on." } else { "Startup off." }, false);
        }
    }

    fn update_draft_tab_name(&mut self, value: String) {
        self.draft_tab_name = value;
    }

    fn update_draft_item_title(&mut self, value: String) {
        self.draft_item_title = value;
    }

    fn update_draft_item_content(&mut self, value: String) {
        self.draft_item_content = value;
    }

    fn copy_item(&mut self, index: usize) -> bool {
        if self.delete_mode {
            return false;
        }

        let Some(tab_index) = self.selected_tab else {
            return false;
        };
        let Some(item) = self
            .config
            .tabs
            .get(tab_index)
            .and_then(|tab| tab.items.get(index))
        else {
            return false;
        };

        match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(item.content.clone())) {
            Ok(()) => {
                self.set_copied_toast("Copied.");
                true
            }
            Err(err) => {
                self.set_status(format!("Copy failed: {err}"), true);
                false
            }
        }
    }

    fn start_window_drag<C: ComponentHandle>(&mut self, window: &C) {
        let Ok((cursor_x, cursor_y)) = current_cursor_position() else {
            return;
        };

        self.drag_origin_cursor = Some((cursor_x, cursor_y));
        self.drag_origin_window = Some(window.window().position());
    }

    fn drag_window<C: ComponentHandle>(&mut self, window: &C) {
        let (Some((origin_cursor_x, origin_cursor_y)), Some(origin_window)) =
            (self.drag_origin_cursor, self.drag_origin_window)
        else {
            return;
        };

        let Ok((cursor_x, cursor_y)) = current_cursor_position() else {
            return;
        };

        window.window().set_position(PhysicalPosition::new(
            origin_window.x + (cursor_x - origin_cursor_x),
            origin_window.y + (cursor_y - origin_cursor_y),
        ));
    }

    fn stop_window_drag(&mut self) {
        self.drag_origin_cursor = None;
        self.drag_origin_window = None;
    }

    fn import_config(&mut self) -> bool {
        let Some(path) = choose_open_path() else {
            return false;
        };

        if !confirm("Import replaces the current local config. Continue?") {
            return false;
        }

        match AppConfig::load_from_path(&path) {
            Ok(config) => {
                let parsed = match parse_hotkey(&config.hotkey) {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        self.set_status(err.to_string(), true);
                        return false;
                    }
                };

                if let Err(err) = self.hotkey_manager.update(parsed.clone()) {
                    self.set_status(err.to_string(), true);
                    return false;
                }

                if let Err(err) = set_launch_at_startup(config.launch_at_startup) {
                    self.set_status(err.to_string(), true);
                    return false;
                }

                self.config = config;
                self.draft_hotkey = parsed.display;
                self.selected_tab = if self.config.tabs.is_empty() {
                    None
                } else {
                    Some(0)
                };
                self.reset_overlay_state();
                self.delete_mode = false;
                self.persist_config();
                if !self.status_is_error {
                    self.set_status("Config imported.", false);
                }
                true
            }
            Err(err) => {
                self.set_status(err.to_string(), true);
                false
            }
        }
    }

    fn export_config(&mut self) {
        let Some(path) = choose_save_path() else {
            return;
        };

        match self.config.save_to_path(&path) {
            Ok(()) => self.set_status("Config exported.", false),
            Err(err) => self.set_status(err.to_string(), true),
        }
    }
}

fn sync_main_and_settings(
    controller: &Rc<RefCell<AppController>>,
    main_weak: &Weak<AppWindow>,
    settings_weak: &Weak<SettingsWindow>,
) {
    if let Some(window) = main_weak.upgrade() {
        controller.borrow().sync_window(&window);
    }
    if let Some(window) = settings_weak.upgrade() {
        controller.borrow().sync_settings_window(&window);
    }
}

fn schedule_status_timeout(
    controller: &Rc<RefCell<AppController>>,
    main_weak: &Weak<AppWindow>,
    settings_weak: &Weak<SettingsWindow>,
) {
    let (status_text, status_is_error, status_revision) = {
        let controller = controller.borrow();
        (
            controller.status_text.clone(),
            controller.status_is_error,
            controller.status_revision,
        )
    };

    if status_text.is_empty() {
        return;
    }

    let timeout = if status_is_error {
        Duration::from_secs(5)
    } else {
        Duration::from_secs(3)
    };

    let controller = controller.clone();
    let main_weak = main_weak.clone();
    let settings_weak = settings_weak.clone();
    Timer::single_shot(timeout, move || {
        let should_clear = {
            let controller = controller.borrow();
            controller.status_revision == status_revision && !controller.status_text.is_empty()
        };

        if !should_clear {
            return;
        }

        controller.borrow_mut().clear_status();
        sync_main_and_settings(&controller, &main_weak, &settings_weak);
    });
}

fn schedule_copied_toast_timeout(
    controller: &Rc<RefCell<AppController>>,
    main_weak: &Weak<AppWindow>,
    settings_weak: &Weak<SettingsWindow>,
) {
    let (copied_toast_text, copied_toast_revision) = {
        let controller = controller.borrow();
        (
            controller.copied_toast_text.clone(),
            controller.copied_toast_revision,
        )
    };

    if copied_toast_text.is_empty() {
        return;
    }

    let controller = controller.clone();
    let main_weak = main_weak.clone();
    let settings_weak = settings_weak.clone();
    Timer::single_shot(Duration::from_millis(1200), move || {
        let should_clear = {
            let controller = controller.borrow();
            controller.copied_toast_revision == copied_toast_revision
                && !controller.copied_toast_text.is_empty()
        };

        if !should_clear {
            return;
        }

        controller.borrow_mut().clear_copied_toast();
        sync_main_and_settings(&controller, &main_weak, &settings_weak);
    });
}

fn wire_callbacks(
    window: &AppWindow,
    settings_window: &SettingsWindow,
    controller: &Rc<RefCell<AppController>>,
) {
    let weak = window.as_weak();
    let settings_weak = settings_window.as_weak();

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_select_tab(move |index| {
            controller.borrow_mut().select_tab(index as usize);
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_request_rename_tab(move |index| {
            controller.borrow_mut().open_rename_tab(index as usize);
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_request_delete_tab(move |index| {
            controller.borrow_mut().request_delete_tab(index as usize);
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        let settings_weak = settings_weak.clone();
        window.on_reorder_tab(move |index, position_x| {
            controller
                .borrow_mut()
                .reorder_tab(index as usize, position_x);
            sync_main_and_settings(&controller, &weak, &settings_weak);
            schedule_status_timeout(&controller, &weak, &settings_weak);
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_start_window_drag(move || {
            if let Some(window) = weak.upgrade() {
                controller.borrow_mut().start_window_drag(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_drag_window(move || {
            if let Some(window) = weak.upgrade() {
                controller.borrow_mut().drag_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        window.on_end_window_drag(move || {
            controller.borrow_mut().stop_window_drag();
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_open_home(move || {
            controller.borrow_mut().open_home();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        let settings_weak = settings_weak.clone();
        window.on_open_add_tab(move || {
            controller.borrow_mut().open_add_tab();
            sync_main_and_settings(&controller, &weak, &settings_weak);
            schedule_status_timeout(&controller, &weak, &settings_weak);
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        let settings_weak = settings_weak.clone();
        window.on_submit_tab_form(move |name| {
            controller.borrow_mut().submit_tab_form(name.to_string());
            sync_main_and_settings(&controller, &weak, &settings_weak);
            schedule_status_timeout(&controller, &weak, &settings_weak);
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        let settings_weak = settings_weak.clone();
        window.on_open_add_item(move || {
            controller.borrow_mut().open_add_item();
            sync_main_and_settings(&controller, &weak, &settings_weak);
            schedule_status_timeout(&controller, &weak, &settings_weak);
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_request_edit_item(move |index| {
            controller.borrow_mut().open_edit_item(index as usize);
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        let settings_weak = settings_weak.clone();
        window.on_submit_item_form(move |title, content| {
            controller
                .borrow_mut()
                .submit_item_form(title.to_string(), content.to_string());
            sync_main_and_settings(&controller, &weak, &settings_weak);
            schedule_status_timeout(&controller, &weak, &settings_weak);
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_toggle_delete_mode(move || {
            controller.borrow_mut().toggle_delete_mode();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_request_delete_item(move |index| {
            controller.borrow_mut().request_delete_item(index as usize);
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        let settings_weak = settings_weak.clone();
        window.on_save_inline_item_edit(move |_index| {
            controller.borrow_mut().save_inline_item_edit();
            sync_main_and_settings(&controller, &weak, &settings_weak);
            schedule_status_timeout(&controller, &weak, &settings_weak);
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_cancel_inline_item_edit(move || {
            controller.borrow_mut().cancel_inline_item_edit();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        let settings_weak = settings_weak.clone();
        window.on_confirm_delete_item(move || {
            controller.borrow_mut().confirm_delete_item();
            sync_main_and_settings(&controller, &weak, &settings_weak);
            schedule_status_timeout(&controller, &weak, &settings_weak);
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        let settings_weak = settings_weak.clone();
        window.on_confirm_delete_tab(move || {
            controller.borrow_mut().confirm_delete_tab();
            sync_main_and_settings(&controller, &weak, &settings_weak);
            schedule_status_timeout(&controller, &weak, &settings_weak);
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_cancel_overlay(move || {
            controller.borrow_mut().cancel_overlay();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_update_draft_tab_name(move |value| {
            controller
                .borrow_mut()
                .update_draft_tab_name(value.to_string());
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_update_draft_item_title(move |value| {
            let should_sync_window = {
                let mut controller = controller.borrow_mut();
                controller.update_draft_item_title(value.to_string());
                // Avoid rebuilding the item list while inline-renaming a card title.
                // Re-syncing the whole window on every keystroke resets the active
                // TextInput state and sends the caret back to the beginning.
                controller.editing_item_index.is_none()
            };

            if should_sync_window {
                if let Some(window) = weak.upgrade() {
                    controller.borrow().sync_window(&window);
                }
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_update_draft_item_content(move |value| {
            controller
                .borrow_mut()
                .update_draft_item_content(value.to_string());
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        let settings_weak = settings_weak.clone();
        window.on_copy_item(move |index| {
            controller.borrow_mut().copy_item(index as usize);
            sync_main_and_settings(&controller, &weak, &settings_weak);
            schedule_status_timeout(&controller, &weak, &settings_weak);
            schedule_copied_toast_timeout(&controller, &weak, &settings_weak);
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        let settings_weak = settings_weak.clone();
        window.on_reorder_item(move |index, position_y| {
            controller
                .borrow_mut()
                .reorder_item(index as usize, position_y);
            sync_main_and_settings(&controller, &weak, &settings_weak);
            schedule_status_timeout(&controller, &weak, &settings_weak);
        });
    }

    {
        let weak = weak.clone();
        window.on_close_app(move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
        });
    }
}

fn wire_settings_callbacks(
    settings_window: &SettingsWindow,
    main_window: &AppWindow,
    controller: &Rc<RefCell<AppController>>,
) {
    let settings_weak = settings_window.as_weak();
    let main_weak = main_window.as_weak();

    {
        let controller = controller.clone();
        let settings_weak = settings_weak.clone();
        let main_weak = main_weak.clone();
        settings_window.on_start_window_drag(move || {
            if let Some(window) = settings_weak.upgrade() {
                controller.borrow_mut().start_window_drag(&window);
            }
            if let Some(window) = main_weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let settings_weak = settings_weak.clone();
        settings_window.on_drag_window(move || {
            if let Some(window) = settings_weak.upgrade() {
                controller.borrow_mut().drag_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        settings_window.on_end_window_drag(move || {
            controller.borrow_mut().stop_window_drag();
        });
    }

    {
        let controller = controller.clone();
        let settings_weak = settings_weak.clone();
        let main_weak = main_weak.clone();
        settings_window.on_prepare_settings(move || {
            controller.borrow_mut().open_settings();
            if let Some(window) = settings_weak.upgrade() {
                controller.borrow().sync_settings_window(&window);
            }
            if let Some(window) = main_weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let settings_weak = settings_weak.clone();
        settings_window.on_close_settings(move || {
            controller.borrow_mut().close_settings();
            if let Some(window) = settings_weak.upgrade() {
                controller.borrow().sync_settings_window(&window);
                let _ = window.hide();
            }
        });
    }

    {
        let controller = controller.clone();
        let settings_weak = settings_weak.clone();
        settings_window.on_begin_hotkey_recording(move || {
            controller.borrow_mut().begin_hotkey_recording();
            if let Some(window) = settings_weak.upgrade() {
                controller.borrow().sync_settings_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let settings_weak = settings_weak.clone();
        settings_window.on_end_hotkey_recording(move || {
            controller.borrow_mut().end_hotkey_recording();
            if let Some(window) = settings_weak.upgrade() {
                controller.borrow().sync_settings_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let settings_weak = settings_weak.clone();
        settings_window.on_capture_hotkey(move |key, alt, ctrl, shift| {
            controller
                .borrow_mut()
                .capture_hotkey(key.to_string(), alt, ctrl, shift);
            if let Some(window) = settings_weak.upgrade() {
                controller.borrow().sync_settings_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let settings_weak = settings_weak.clone();
        let main_weak = main_weak.clone();
        settings_window.on_toggle_launch_at_startup(move || {
            controller.borrow_mut().toggle_launch_at_startup();
            sync_main_and_settings(&controller, &main_weak, &settings_weak);
            schedule_status_timeout(&controller, &main_weak, &settings_weak);
        });
    }

    {
        let controller = controller.clone();
        let settings_weak = settings_weak.clone();
        let main_weak = main_weak.clone();
        settings_window.on_save_hotkey(move |value| {
            controller.borrow_mut().save_hotkey(value.to_string());
            sync_main_and_settings(&controller, &main_weak, &settings_weak);
            schedule_status_timeout(&controller, &main_weak, &settings_weak);
        });
    }

    {
        let controller = controller.clone();
        let settings_weak = settings_weak.clone();
        let main_weak = main_weak.clone();
        settings_window.on_trigger_import(move || {
            let imported = controller.borrow_mut().import_config();
            if let Some(window) = settings_weak.upgrade() {
                controller.borrow().sync_settings_window(&window);
                if imported {
                    let _ = window.hide();
                }
            }
            if let Some(window) = main_weak.upgrade() {
                controller.borrow().sync_window(&window);
                if imported {
                    let _ = show_component_window(
                        &window,
                        HOME_WINDOW_TITLE,
                        (HOME_WINDOW_WIDTH, HOME_WINDOW_HEIGHT),
                    );
                }
            }
            schedule_status_timeout(&controller, &main_weak, &settings_weak);
        });
    }

    {
        let controller = controller.clone();
        let settings_weak = settings_weak.clone();
        let main_weak = main_weak.clone();
        settings_window.on_trigger_export(move || {
            controller.borrow_mut().export_config();
            sync_main_and_settings(&controller, &main_weak, &settings_weak);
            schedule_status_timeout(&controller, &main_weak, &settings_weak);
        });
    }
}

fn toggle_window_visibility(main_weak: &Weak<AppWindow>, settings_weak: &Weak<SettingsWindow>) {
    let weak = main_weak.clone();
    let settings_weak = settings_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(settings_window) = settings_weak.upgrade() {
            let _ = settings_window.hide();
        }
        if let Some(window) = weak.upgrade() {
            if window.window().is_visible() {
                let _ = window.hide();
            } else {
                window.invoke_open_home();
                let _ = show_component_window(
                    &window,
                    HOME_WINDOW_TITLE,
                    (HOME_WINDOW_WIDTH, HOME_WINDOW_HEIGHT),
                );
            }
        }
    });
}

fn show_home_from_tray(main_weak: &Weak<AppWindow>, settings_weak: &Weak<SettingsWindow>) {
    let weak = main_weak.clone();
    let settings_weak = settings_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = weak.upgrade() {
            window.invoke_open_home();
            if show_component_window(
                &window,
                HOME_WINDOW_TITLE,
                (HOME_WINDOW_WIDTH, HOME_WINDOW_HEIGHT),
            )
            .is_ok()
            {
                if let Some(settings_window) = settings_weak.upgrade() {
                    let _ = settings_window.hide();
                }
            }
        }
    });
}

fn show_settings_from_tray(main_weak: &Weak<AppWindow>, settings_weak: &Weak<SettingsWindow>) {
    let weak = main_weak.clone();
    let settings_weak = settings_weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(settings_window) = settings_weak.upgrade() {
            settings_window.invoke_prepare_settings();
            if show_component_window(
                &settings_window,
                SETTINGS_WINDOW_TITLE,
                (SETTINGS_WINDOW_WIDTH, SETTINGS_WINDOW_HEIGHT),
            )
            .is_ok()
            {
                if let Some(window) = weak.upgrade() {
                    let _ = window.hide();
                }
                return;
            }
        }

        if let Some(window) = weak.upgrade() {
            window.invoke_open_home();
            let _ = show_component_window(
                &window,
                HOME_WINDOW_TITLE,
                (HOME_WINDOW_WIDTH, HOME_WINDOW_HEIGHT),
            );
        }
    });
}

fn quit_from_tray() {
    let _ = slint::invoke_from_event_loop(move || {
        let _ = slint::quit_event_loop();
    });
}

fn fit_window_dimension(design: i32, available: i32, scale_factor: f32, outer_padding: i32) -> f32 {
    if available <= 0 || scale_factor <= 0.0 {
        return 1.0;
    }

    let available_physical = (available - outer_padding).max(1) as f32;
    let available_logical = available_physical / scale_factor;
    let safe_margin_logical = WINDOW_SAFE_MARGIN as f32 / scale_factor;
    let usable = if available_logical > safe_margin_logical * 2.0 {
        available_logical - safe_margin_logical * 2.0
    } else {
        available_logical
    };

    (design as f32).min(usable).max(1.0)
}

fn compute_window_size(
    design_size: (i32, i32),
    work_area: (i32, i32, i32, i32),
    scale_factor: f32,
    outer_padding: (i32, i32),
) -> LogicalSize {
    let work_width = (work_area.2 - work_area.0).max(1);
    let work_height = (work_area.3 - work_area.1).max(1);

    LogicalSize::new(
        fit_window_dimension(design_size.0, work_width, scale_factor, outer_padding.0),
        fit_window_dimension(design_size.1, work_height, scale_factor, outer_padding.1),
    )
}

fn outer_padding_from_rect(
    inner_size: slint::PhysicalSize,
    outer_rect: (i32, i32, i32, i32),
) -> (i32, i32) {
    (
        (outer_rect.2 - outer_rect.0 - inner_size.width as i32).max(0),
        (outer_rect.3 - outer_rect.1 - inner_size.height as i32).max(0),
    )
}

fn rect_size_for_logical_size(
    logical_size: LogicalSize,
    scale_factor: f32,
    outer_padding: (i32, i32),
) -> (i32, i32) {
    let physical = logical_size.to_physical(scale_factor);
    (
        physical.width as i32 + outer_padding.0,
        physical.height as i32 + outer_padding.1,
    )
}

fn compute_preferred_window_position(
    rect_size: (i32, i32),
    work_area: (i32, i32, i32, i32),
) -> PhysicalPosition {
    let width = rect_size.0.max(1);
    let height = rect_size.1.max(1);
    let (_, top, right, bottom) = work_area;
    let centered_y = top + ((bottom - top - height) / 2);

    PhysicalPosition::new(right - width - WINDOW_SAFE_MARGIN, centered_y)
}

fn clamp_window_position(
    preferred_position: PhysicalPosition,
    rect_size: (i32, i32),
    work_area: (i32, i32, i32, i32),
) -> PhysicalPosition {
    let (left, top, right, bottom) = work_area;
    let width = rect_size.0.max(1);
    let height = rect_size.1.max(1);

    let min_x = left + WINDOW_SAFE_MARGIN;
    let max_x = right - width - WINDOW_SAFE_MARGIN;
    let x = if max_x >= min_x {
        preferred_position.x.clamp(min_x, max_x)
    } else {
        (right - width).max(left)
    };

    let min_y = top + WINDOW_SAFE_MARGIN;
    let max_y = bottom - height - WINDOW_SAFE_MARGIN;
    let y = if max_y >= min_y {
        preferred_position.y.clamp(min_y, max_y)
    } else {
        (bottom - height).max(top)
    };

    PhysicalPosition::new(x, y)
}

fn position_window_for_work_area(
    rect_size: (i32, i32),
    work_area: (i32, i32, i32, i32),
) -> PhysicalPosition {
    clamp_window_position(
        compute_preferred_window_position(rect_size, work_area),
        rect_size,
        work_area,
    )
}

fn show_component_window<C: ComponentHandle + 'static>(
    window: &C,
    title: &str,
    fallback_size: (i32, i32),
) -> Result<(), slint::PlatformError> {
    let initial_work_area = current_monitor_work_area().ok();
    let initial_scale_factor = current_monitor_scale_factor().ok().unwrap_or(1.0);
    let desired_size = initial_work_area
        .map(|area| compute_window_size(fallback_size, area, initial_scale_factor, (0, 0)))
        .unwrap_or_else(|| {
            LogicalSize::new(fallback_size.0.max(1) as f32, fallback_size.1.max(1) as f32)
        });

    window.window().set_minimized(false);
    window.window().set_size(desired_size);
    if let Some(area) = initial_work_area {
        let desired_physical_size = desired_size.to_physical(initial_scale_factor);
        window.window().set_position(position_window_for_work_area(
            (
                desired_physical_size.width as i32,
                desired_physical_size.height as i32,
            ),
            area,
        ));
    }
    if !window.window().is_visible() {
        window.show()?;
    }

    let _ = apply_app_icon_to_window(title);
    let _ = bring_window_to_front(title);

    let actual_work_area = current_window_work_area(title)
        .ok()
        .flatten()
        .or(initial_work_area);
    let actual_scale_factor = current_window_scale_factor(title)
        .ok()
        .flatten()
        .unwrap_or_else(|| window.window().scale_factor().max(1.0));

    if let Some(area) = actual_work_area {
        let current_inner_size = window.window().size();
        let observed_outer_padding = current_window_rect(title)
            .ok()
            .flatten()
            .map(|rect| outer_padding_from_rect(current_inner_size, rect))
            .unwrap_or((0, 0));
        let corrected_size = compute_window_size(
            fallback_size,
            area,
            actual_scale_factor,
            observed_outer_padding,
        );
        let current_size = LogicalSize::from_physical(current_inner_size, actual_scale_factor);
        if (current_size.width - corrected_size.width).abs() > 0.5
            || (current_size.height - corrected_size.height).abs() > 0.5
        {
            window.window().set_size(corrected_size);
        }

        let rect_size =
            rect_size_for_logical_size(corrected_size, actual_scale_factor, observed_outer_padding);

        window
            .window()
            .set_position(position_window_for_work_area(rect_size, area));
    }

    Ok(())
}

fn drop_target_index(position: f32, slot_size: f32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }

    let snapped = if position <= 0.0 {
        0
    } else {
        (position / slot_size).floor() as usize
    };

    snapped.min(len.saturating_sub(1))
}

fn clamp_selected_tab(index: usize, new_len: usize) -> Option<usize> {
    if new_len == 0 {
        None
    } else if index >= new_len {
        Some(new_len - 1)
    } else {
        Some(index)
    }
}

fn is_modifier_key(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "shift" | "control" | "ctrl" | "alt" | "meta" | "super"
    )
}

fn build_hotkey_candidate(
    key_text: &str,
    alt: bool,
    ctrl: bool,
    shift: bool,
) -> Result<String, AppError> {
    let key_name = normalize_hotkey_key(key_text)
        .ok_or_else(|| AppError::validation("Use letters, digits, or F1-F12."))?;

    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl".to_string());
    }
    if alt {
        parts.push("Alt".to_string());
    }
    if shift {
        parts.push("Shift".to_string());
    }

    if parts.is_empty() {
        return Err(AppError::validation(
            "Hotkey needs at least one modifier key.",
        ));
    }

    parts.push(key_name);
    Ok(parts.join("+"))
}

fn normalize_hotkey_key(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let upper = trimmed.to_ascii_uppercase();
    if upper.len() == 1 && upper.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Some(upper);
    }

    if let Some(number) = upper.strip_prefix('F') {
        let function = number.parse::<u8>().ok()?;
        if (1..=12).contains(&function) {
            return Some(format!("F{function}"));
        }
    }

    None
}

fn preview_text(value: &str) -> String {
    const MAX_PREVIEW_LEN: usize = 100;

    let compact = value
        .split_whitespace()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if compact.chars().count() <= MAX_PREVIEW_LEN {
        compact
    } else {
        let truncated = compact.chars().take(MAX_PREVIEW_LEN).collect::<String>();
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use slint::{LogicalSize, PhysicalPosition};

    use super::{
        build_hotkey_candidate, clamp_window_position, compute_preferred_window_position,
        compute_window_size, normalize_hotkey_key, position_window_for_work_area,
    };

    #[test]
    fn normalize_hotkey_key_accepts_single_letter() {
        assert_eq!(normalize_hotkey_key("v").as_deref(), Some("V"));
    }

    #[test]
    fn build_hotkey_candidate_uses_modifier_flags() {
        let hotkey = build_hotkey_candidate("v", true, false, false).expect("Alt+V should build");
        assert_eq!(hotkey, "Alt+V");
    }

    #[test]
    fn compute_window_size_preserves_design_when_work_area_is_large() {
        let size = compute_window_size((440, 664), (0, 0, 1920, 1080), 1.0, (0, 0));
        assert_eq!(size, LogicalSize::new(440.0, 664.0));
    }

    #[test]
    fn compute_window_size_shrinks_to_fit_work_area() {
        let size = compute_window_size((440, 664), (0, 0, 360, 500), 1.0, (0, 0));
        assert_eq!(size, LogicalSize::new(328.0, 468.0));
    }

    #[test]
    fn compute_window_size_preserves_design_in_scaled_physical_work_area() {
        let size = compute_window_size((440, 664), (0, 0, 1920, 1080), 1.25, (0, 0));
        assert_eq!(size, LogicalSize::new(440.0, 664.0));
    }

    #[test]
    fn compute_window_size_accounts_for_outer_padding() {
        let size = compute_window_size((440, 664), (0, 0, 360, 500), 1.0, (0, 24));
        assert_eq!(size, LogicalSize::new(328.0, 444.0));
    }

    #[test]
    fn compute_preferred_window_position_anchors_to_right_center() {
        let position = compute_preferred_window_position((440, 664), (0, 0, 1200, 900));
        assert_eq!(position.x, 744);
        assert_eq!(position.y, 118);
    }

    #[test]
    fn clamp_window_position_keeps_rect_inside_work_area() {
        let position = clamp_window_position(
            PhysicalPosition::new(900, -50),
            (440, 664),
            (0, 0, 1200, 900),
        );
        assert_eq!(position.x, 744);
        assert_eq!(position.y, 16);
    }

    #[test]
    fn position_window_for_work_area_uses_actual_rect_size() {
        let position = position_window_for_work_area((470, 664), (0, 0, 1200, 900));
        assert_eq!(position.x, 714);
        assert_eq!(position.y, 118);
    }
}
