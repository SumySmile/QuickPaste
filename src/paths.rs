use std::env;
use std::path::{Path, PathBuf};

use crate::error::AppError;

pub const APP_DIRECTORY_NAME: &str = "MyQuickPaste";
pub const CONFIG_FILE_NAME: &str = "quick-paste.toml";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_path: PathBuf,
    pub portable_mode: bool,
}

impl AppPaths {
    pub fn discover() -> Result<Self, AppError> {
        let exe_dir = current_exe_dir()?;
        let portable_path = exe_dir.join(CONFIG_FILE_NAME);

        if portable_path.exists() {
            return Ok(Self {
                config_path: portable_path,
                portable_mode: true,
            });
        }

        if let Some(roaming_path) = roaming_config_path() {
            return Ok(Self {
                config_path: roaming_path,
                portable_mode: false,
            });
        }

        Ok(Self {
            config_path: portable_path,
            portable_mode: true,
        })
    }
}

fn current_exe_dir() -> Result<PathBuf, AppError> {
    let exe_path = env::current_exe()?;
    let parent = exe_path.parent().ok_or_else(|| {
        AppError::validation("Unable to determine executable directory for config discovery.")
    })?;

    Ok(parent.to_path_buf())
}

fn roaming_config_path() -> Option<PathBuf> {
    env::var_os("APPDATA").map(|appdata| {
        Path::new(&appdata)
            .join(APP_DIRECTORY_NAME)
            .join(CONFIG_FILE_NAME)
    })
}

#[cfg(test)]
mod tests {
    use super::{AppPaths, CONFIG_FILE_NAME};

    #[test]
    fn config_path_ends_with_expected_file_name() {
        let paths = AppPaths::discover().expect("paths should resolve");
        assert_eq!(
            paths
                .config_path
                .file_name()
                .and_then(|value| value.to_str()),
            Some(CONFIG_FILE_NAME)
        );
    }
}
