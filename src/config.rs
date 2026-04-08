use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

pub const CURRENT_CONFIG_VERSION: u32 = 1;
pub const DEFAULT_HOTKEY: &str = "Alt+V";
pub const MAX_TABS: usize = 4;
pub const MAX_ITEMS_PER_TAB: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_config_version")]
    pub version: u32,
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default)]
    pub launch_at_startup: bool,
    #[serde(default)]
    pub tabs: Vec<TabConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabConfig {
    pub name: String,
    #[serde(default)]
    pub items: Vec<ItemConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemConfig {
    pub title: String,
    pub content: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CURRENT_CONFIG_VERSION,
            hotkey: DEFAULT_HOTKEY.to_string(),
            launch_at_startup: false,
            tabs: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn load_from_path(path: &Path) -> Result<Self, AppError> {
        let content = fs::read_to_string(path)?;
        Self::from_toml_str(&content)
    }

    pub fn load_or_default(path: &Path) -> Result<Self, AppError> {
        if !path.exists() {
            return Ok(Self::default());
        }

        Self::load_from_path(path)
    }

    pub fn from_toml_str(content: &str) -> Result<Self, AppError> {
        let config: Self = toml::from_str(content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if self.version != CURRENT_CONFIG_VERSION {
            return Err(AppError::validation(format!(
                "Unsupported config version: {}",
                self.version
            )));
        }

        if self.hotkey.trim().is_empty() {
            return Err(AppError::validation("Hotkey cannot be empty."));
        }

        if self.tabs.len() > MAX_TABS {
            return Err(AppError::validation(format!(
                "Too many tabs: {} (max {}).",
                self.tabs.len(),
                MAX_TABS
            )));
        }

        for tab in &self.tabs {
            if tab.name.trim().is_empty() {
                return Err(AppError::validation("Tab name cannot be empty."));
            }

            if tab.items.len() > MAX_ITEMS_PER_TAB {
                return Err(AppError::validation(format!(
                    "Too many items in tab '{}': {} (max {}).",
                    tab.name,
                    tab.items.len(),
                    MAX_ITEMS_PER_TAB
                )));
            }

            for item in &tab.items {
                if item.title.trim().is_empty() {
                    return Err(AppError::validation(format!(
                        "Item title cannot be empty in tab '{}'.",
                        tab.name
                    )));
                }

                if item.content.trim().is_empty() {
                    return Err(AppError::validation(format!(
                        "Item content cannot be empty for '{}' in tab '{}'.",
                        item.title, tab.name
                    )));
                }
            }
        }

        Ok(())
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), AppError> {
        self.validate()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(path, self.to_toml_string()?)?;
        Ok(())
    }

    pub fn to_toml_string(&self) -> Result<String, AppError> {
        self.validate()?;
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn add_tab(&mut self, name: impl Into<String>) -> Result<usize, AppError> {
        if self.tabs.len() >= MAX_TABS {
            return Err(AppError::validation(format!(
                "Too many tabs: {} (max {}).",
                self.tabs.len(),
                MAX_TABS
            )));
        }

        let name = name.into();
        if name.trim().is_empty() {
            return Err(AppError::validation("Tab name cannot be empty."));
        }

        self.tabs.push(TabConfig {
            name,
            items: Vec::new(),
        });
        self.validate()?;
        Ok(self.tabs.len() - 1)
    }

    pub fn remove_tab(&mut self, index: usize) -> Result<(), AppError> {
        if index >= self.tabs.len() {
            return Err(AppError::validation("Tab index out of range."));
        }

        self.tabs.remove(index);
        self.validate()?;
        Ok(())
    }

    pub fn move_tab(&mut self, from_index: usize, to_index: usize) -> Result<(), AppError> {
        if from_index >= self.tabs.len() || to_index >= self.tabs.len() {
            return Err(AppError::validation("Tab index out of range."));
        }

        if from_index == to_index {
            return Ok(());
        }

        let tab = self.tabs.remove(from_index);
        self.tabs.insert(to_index, tab);
        self.validate()?;
        Ok(())
    }

    pub fn add_item(
        &mut self,
        tab_index: usize,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<(), AppError> {
        let tab = self
            .tabs
            .get_mut(tab_index)
            .ok_or_else(|| AppError::validation("Tab index out of range."))?;

        if tab.items.len() >= MAX_ITEMS_PER_TAB {
            return Err(AppError::validation(format!(
                "Too many items in tab '{}': {} (max {}).",
                tab.name,
                tab.items.len(),
                MAX_ITEMS_PER_TAB
            )));
        }

        tab.items.push(ItemConfig {
            title: title.into(),
            content: content.into(),
        });
        self.validate()?;
        Ok(())
    }

    pub fn remove_item(&mut self, tab_index: usize, item_index: usize) -> Result<(), AppError> {
        let tab = self
            .tabs
            .get_mut(tab_index)
            .ok_or_else(|| AppError::validation("Tab index out of range."))?;

        if item_index >= tab.items.len() {
            return Err(AppError::validation("Item index out of range."));
        }

        tab.items.remove(item_index);
        self.validate()?;
        Ok(())
    }

    pub fn move_item(
        &mut self,
        tab_index: usize,
        from_index: usize,
        to_index: usize,
    ) -> Result<(), AppError> {
        let tab = self
            .tabs
            .get_mut(tab_index)
            .ok_or_else(|| AppError::validation("Tab index out of range."))?;

        if from_index >= tab.items.len() || to_index >= tab.items.len() {
            return Err(AppError::validation("Item index out of range."));
        }

        if from_index == to_index {
            return Ok(());
        }

        let item = tab.items.remove(from_index);
        tab.items.insert(to_index, item);
        self.validate()?;
        Ok(())
    }
}

fn default_config_version() -> u32 {
    CURRENT_CONFIG_VERSION
}

fn default_hotkey() -> String {
    DEFAULT_HOTKEY.to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        AppConfig, CURRENT_CONFIG_VERSION, DEFAULT_HOTKEY, ItemConfig, MAX_ITEMS_PER_TAB, MAX_TABS,
        TabConfig,
    };

    #[test]
    fn default_config_is_valid() {
        let config = AppConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.version, CURRENT_CONFIG_VERSION);
        assert_eq!(config.hotkey, DEFAULT_HOTKEY);
        assert!(!config.launch_at_startup);
        assert!(config.tabs.is_empty());
    }

    #[test]
    fn sample_toml_round_trip_stays_valid() {
        let sample = r#"
version = 1
hotkey = "Alt+V"
launch_at_startup = true

[[tabs]]
name = "URL"

[[tabs.items]]
title = "Home"
content = "https://example.com"
"#;

        let config = AppConfig::from_toml_str(sample).expect("sample should parse");
        assert!(config.launch_at_startup);
        let rendered = config.to_toml_string().expect("config should render");
        let reparsed = AppConfig::from_toml_str(&rendered).expect("rendered config should parse");

        assert_eq!(reparsed, config);
    }

    #[test]
    fn missing_file_uses_default_config() {
        let path = unique_temp_path("missing");
        let config = AppConfig::load_or_default(&path).expect("missing config should fall back");

        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn save_and_reload_round_trip() {
        let path = unique_temp_path("roundtrip");
        let mut config = AppConfig::default();
        config
            .add_tab("Accounts")
            .expect("tab should be added successfully");
        config
            .add_item(0, "Admin", "admin@example.com")
            .expect("item should be added successfully");

        config.save_to_path(&path).expect("config should save");
        let loaded = AppConfig::load_or_default(&path).expect("config should load");
        assert_eq!(loaded, config);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn add_and_remove_tab_and_item() {
        let mut config = AppConfig::default();
        let tab_index = config.add_tab("URL").expect("tab should be added");
        config
            .add_item(tab_index, "Home", "https://example.com")
            .expect("item should be added");
        config
            .remove_item(tab_index, 0)
            .expect("item should be removed");
        config.remove_tab(tab_index).expect("tab should be removed");

        assert!(config.tabs.is_empty());
    }

    #[test]
    fn validate_rejects_too_many_tabs() {
        let mut config = AppConfig::default();
        config.tabs = (0..=MAX_TABS)
            .map(|index| TabConfig {
                name: format!("Tab {index}"),
                items: Vec::new(),
            })
            .collect();

        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_too_many_items() {
        let mut config = AppConfig::default();
        config.tabs.push(TabConfig {
            name: "Main".to_string(),
            items: (0..=MAX_ITEMS_PER_TAB)
                .map(|index| ItemConfig {
                    title: format!("Item {index}"),
                    content: format!("Value {index}"),
                })
                .collect(),
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn remove_item_rejects_out_of_range_index() {
        let mut config = AppConfig::default();
        let tab_index = config.add_tab("URL").expect("tab should be added");
        config
            .add_item(tab_index, "Home", "https://example.com")
            .expect("item should be added");

        assert!(config.remove_item(tab_index, 1).is_err());
    }

    #[test]
    fn move_tab_reorders_tabs() {
        let mut config = AppConfig::default();
        config.add_tab("A").expect("tab a");
        config.add_tab("B").expect("tab b");
        config.add_tab("C").expect("tab c");

        config.move_tab(0, 2).expect("move tab");

        let names = config
            .tabs
            .iter()
            .map(|tab| tab.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["B", "C", "A"]);
    }

    #[test]
    fn move_item_reorders_items() {
        let mut config = AppConfig::default();
        let tab_index = config.add_tab("Main").expect("tab main");
        config.add_item(tab_index, "A", "1").expect("item a");
        config.add_item(tab_index, "B", "2").expect("item b");
        config.add_item(tab_index, "C", "3").expect("item c");

        config.move_item(tab_index, 2, 0).expect("move item");

        let titles = config.tabs[tab_index]
            .items
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(titles, vec!["C", "A", "B"]);
    }

    fn unique_temp_path(label: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be valid")
            .as_nanos();

        path.push(format!("myquickpaste-slint-{label}-{nonce}.toml"));
        path
    }
}
