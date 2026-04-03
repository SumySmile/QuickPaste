use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum AppError {
    Io(std::io::Error),
    TomlDeserialize(toml::de::Error),
    TomlSerialize(toml::ser::Error),
    Validation(String),
    UnsupportedPlatform(&'static str),
}

impl AppError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}

impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::TomlDeserialize(err) => write!(f, "Config parse error: {err}"),
            Self::TomlSerialize(err) => write!(f, "Config write error: {err}"),
            Self::Validation(message) => write!(f, "{message}"),
            Self::UnsupportedPlatform(message) => write!(f, "{message}"),
        }
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::TomlDeserialize(err) => Some(err),
            Self::TomlSerialize(err) => Some(err),
            Self::Validation(_) | Self::UnsupportedPlatform(_) => None,
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<toml::de::Error> for AppError {
    fn from(value: toml::de::Error) -> Self {
        Self::TomlDeserialize(value)
    }
}

impl From<toml::ser::Error> for AppError {
    fn from(value: toml::ser::Error) -> Self {
        Self::TomlSerialize(value)
    }
}
