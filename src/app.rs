use std::cell::RefCell;
use std::rc::Rc;

use arboard::Clipboard;
use slint::{CloseRequestResponse, ComponentHandle, ModelRc, PhysicalPosition, VecModel, Weak};

use crate::config::{AppConfig, MAX_ITEMS_PER_TAB, MAX_TABS};
use crate::error::AppError;
use crate::paths::AppPaths;
use crate::platform::{
    HotkeyManager, TrayIconManager, choose_open_path, choose_save_path, confirm,
    current_cursor_position, parse_hotkey, set_launch_at_startup,
};
use crate::{ActiveView, AppWindow, ItemViewData, OverlayKind, TabViewData};

const TAB_DROP_SLOT_WIDTH: f32 = 92.0;
const ITEM_DROP_SLOT_HEIGHT: f32 = 104.0;

pub fn run() -> Result<(), AppError> {
    let paths = AppPaths::discover()?;
    let config = AppConfig::load_or_default(&paths.config_path)?;
    let hotkey = parse_hotkey(&config.hotkey)?;

    let window = AppWindow::new()
        .map_err(|err| AppError::validation(format!("Failed to create the Slint window: {err}")))?;

    window
        .window()
        .on_close_requested(|| CloseRequestResponse::HideWindow);

    let hotkey_weak = window.as_weak();
    let hotkey_manager =
        HotkeyManager::start(hotkey, move || toggle_window_visibility(&hotkey_weak))?;

    let controller = Rc::new(RefCell::new(AppController::new(
        paths,
        config,
        hotkey_manager,
    )));
    controller.borrow().sync_window(&window);
    wire_callbacks(&window, &controller);

    let tray_weak = window.as_weak();
    let _tray_manager = TrayIconManager::start(
        {
            let weak = tray_weak.clone();
            move || show_home_from_tray(&weak)
        },
        {
            let weak = tray_weak.clone();
            move || show_settings_from_tray(&weak)
        },
        move || quit_from_tray(),
    )?;

    window
        .show()
        .map_err(|err| AppError::validation(format!("Failed to show the Slint window: {err}")))?;

    slint::run_event_loop_until_quit()
        .map_err(|err| AppError::validation(format!("Failed to run the Slint event loop: {err}")))
}

struct AppController {
    paths: AppPaths,
    config: AppConfig,
    hotkey_manager: HotkeyManager,
    selected_tab: Option<usize>,
    active_view: ActiveView,
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
            active_view: ActiveView::Home,
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
        };

        if let Err(err) = set_launch_at_startup(controller.config.launch_at_startup) {
            controller.set_status(err.to_string(), true);
        }

        controller
    }

    fn sync_window(&self, window: &AppWindow) {
        window.set_active_view(self.active_view);
        window.set_overlay(self.overlay);
        window.set_delete_mode(self.delete_mode);
        window.set_has_tabs(!self.config.tabs.is_empty());
        window.set_has_current_tab(self.selected_tab.is_some());
        window.set_status_text(self.status_text.clone().into());
        window.set_status_is_error(self.status_is_error);
        window.set_hotkey(self.config.hotkey.clone().into());
        window.set_hotkey_recording(self.hotkey_recording);
        window.set_launch_at_startup(self.config.launch_at_startup);
        window.set_draft_hotkey(self.draft_hotkey.clone().into());
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
            .map(|item| ItemViewData {
                title: item.title.clone().into(),
                preview: preview_text(&item.content).into(),
                can_delete: self.delete_mode,
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
    }

    fn clear_status(&mut self) {
        self.status_text.clear();
        self.status_is_error = false;
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
        self.active_view = ActiveView::Settings;
        self.reset_overlay_state();
        self.delete_mode = false;
        self.draft_hotkey = self.config.hotkey.clone();
        self.hotkey_recording = false;
        self.clear_status();
    }

    fn open_home(&mut self) {
        self.active_view = ActiveView::Home;
        self.reset_overlay_state();
        self.delete_mode = false;
        self.stop_window_drag();
        self.clear_status();
    }

    fn close_settings(&mut self) {
        self.active_view = ActiveView::Home;
        self.reset_overlay_state();
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
        let Some((title, content)) = self
            .config
            .tabs
            .get(tab_index)
            .and_then(|tab| tab.items.get(index))
            .map(|item| (item.title.clone(), item.content.clone()))
        else {
            return;
        };

        self.reset_overlay_state();
        self.overlay = OverlayKind::EditItem;
        self.editing_item_index = Some(index);
        self.draft_item_title = title;
        self.draft_item_content = content;
        self.clear_status();
    }

    fn submit_item_form(&mut self, title: String, content: String) {
        match self.overlay {
            OverlayKind::AddItem => self.add_item(title, content),
            OverlayKind::EditItem => self.edit_item(title, content),
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

    fn edit_item(&mut self, title: String, content: String) {
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
        item.content = content.trim().to_string();

        match self.config.validate() {
            Ok(()) => {
                self.reset_overlay_state();
                self.persist_config();
                if !self.status_is_error {
                    self.set_status("Item updated.", false);
                }
            }
            Err(err) => self.set_status(err.to_string(), true),
        }
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

        let target_index = drop_target_index(position_y - 12.0, ITEM_DROP_SLOT_HEIGHT, item_count);
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

    fn update_draft_hotkey(&mut self, value: String) {
        self.draft_hotkey = value;
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
                self.set_status("Copied.", false);
                true
            }
            Err(err) => {
                self.set_status(format!("Copy failed: {err}"), true);
                false
            }
        }
    }

    fn start_window_drag(&mut self, window: &AppWindow) {
        let Ok((cursor_x, cursor_y)) = current_cursor_position() else {
            return;
        };

        self.drag_origin_cursor = Some((cursor_x, cursor_y));
        self.drag_origin_window = Some(window.window().position());
    }

    fn drag_window(&mut self, window: &AppWindow) {
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

    fn import_config(&mut self) {
        let Some(path) = choose_open_path() else {
            return;
        };

        if !confirm("Import replaces the current local config. Continue?") {
            return;
        }

        match AppConfig::load_from_path(&path) {
            Ok(config) => {
                let parsed = match parse_hotkey(&config.hotkey) {
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

                if let Err(err) = set_launch_at_startup(config.launch_at_startup) {
                    self.set_status(err.to_string(), true);
                    return;
                }

                self.config = config;
                self.draft_hotkey = parsed.display;
                self.selected_tab = if self.config.tabs.is_empty() {
                    None
                } else {
                    Some(0)
                };
                self.active_view = ActiveView::Home;
                self.reset_overlay_state();
                self.delete_mode = false;
                self.persist_config();
                if !self.status_is_error {
                    self.set_status("Config imported.", false);
                }
            }
            Err(err) => self.set_status(err.to_string(), true),
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

fn wire_callbacks(window: &AppWindow, controller: &Rc<RefCell<AppController>>) {
    let weak = window.as_weak();

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
        window.on_reorder_tab(move |index, position_x| {
            controller
                .borrow_mut()
                .reorder_tab(index as usize, position_x);
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
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
        window.on_begin_hotkey_recording(move || {
            controller.borrow_mut().begin_hotkey_recording();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_end_hotkey_recording(move || {
            controller.borrow_mut().end_hotkey_recording();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_capture_hotkey(move |key, alt, ctrl, shift| {
            controller
                .borrow_mut()
                .capture_hotkey(key.to_string(), alt, ctrl, shift);
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_open_settings(move || {
            controller.borrow_mut().open_settings();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_close_settings(move || {
            controller.borrow_mut().close_settings();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_open_add_tab(move || {
            controller.borrow_mut().open_add_tab();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_submit_tab_form(move |name| {
            controller.borrow_mut().submit_tab_form(name.to_string());
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_open_add_item(move || {
            controller.borrow_mut().open_add_item();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
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
        window.on_submit_item_form(move |title, content| {
            controller
                .borrow_mut()
                .submit_item_form(title.to_string(), content.to_string());
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
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
        window.on_confirm_delete_item(move || {
            controller.borrow_mut().confirm_delete_item();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_confirm_delete_tab(move || {
            controller.borrow_mut().confirm_delete_tab();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
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
        window.on_toggle_launch_at_startup(move || {
            controller.borrow_mut().toggle_launch_at_startup();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_update_draft_hotkey(move |value| {
            controller
                .borrow_mut()
                .update_draft_hotkey(value.to_string());
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
            controller
                .borrow_mut()
                .update_draft_item_title(value.to_string());
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
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
        window.on_save_hotkey(move |value| {
            controller.borrow_mut().save_hotkey(value.to_string());
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_copy_item(move |index| {
            controller.borrow_mut().copy_item(index as usize);
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_reorder_item(move |index, position_y| {
            controller
                .borrow_mut()
                .reorder_item(index as usize, position_y);
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_trigger_import(move || {
            controller.borrow_mut().import_config();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
        });
    }

    {
        let controller = controller.clone();
        let weak = weak.clone();
        window.on_trigger_export(move || {
            controller.borrow_mut().export_config();
            if let Some(window) = weak.upgrade() {
                controller.borrow().sync_window(&window);
            }
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

fn toggle_window_visibility(weak: &Weak<AppWindow>) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = weak.upgrade() {
            if window.window().is_visible() {
                let _ = window.hide();
            } else {
                let _ = window.show();
            }
        }
    });
}

fn show_home_from_tray(weak: &Weak<AppWindow>) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = weak.upgrade() {
            window.invoke_open_home();
            let _ = window.show();
        }
    });
}

fn show_settings_from_tray(weak: &Weak<AppWindow>) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = weak.upgrade() {
            window.invoke_open_settings();
            let _ = window.show();
        }
    });
}

fn quit_from_tray() {
    let _ = slint::invoke_from_event_loop(move || {
        let _ = slint::quit_event_loop();
    });
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
    use super::{build_hotkey_candidate, normalize_hotkey_key};

    #[test]
    fn normalize_hotkey_key_accepts_single_letter() {
        assert_eq!(normalize_hotkey_key("v").as_deref(), Some("V"));
    }

    #[test]
    fn build_hotkey_candidate_uses_modifier_flags() {
        let hotkey = build_hotkey_candidate("v", true, false, false).expect("Alt+V should build");
        assert_eq!(hotkey, "Alt+V");
    }
}

