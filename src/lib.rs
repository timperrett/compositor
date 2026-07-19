pub const BUILD_VERSION: &str = env!("COMPOSITOR_BUILD_VERSION");

pub(crate) mod art;
pub(crate) mod art_brief;
pub mod assets;
pub mod build;
pub mod cli;
pub mod composition;
pub mod config;
pub(crate) mod diff;
pub mod discovery;
pub mod flow;
pub(crate) mod identity;
pub(crate) mod manifest;
pub(crate) mod markdown;
pub mod model;
pub mod overrides;
pub mod package;
pub(crate) mod planning;
pub(crate) mod proof;
pub(crate) mod report;
pub mod storage;
pub(crate) mod text;
pub(crate) mod validation;

use std::path::PathBuf;

/// Failures while loading or validating project configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration file could not be read at {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("configuration at {path} is invalid")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("configuration error: {message}")]
    Invalid { message: String },
}

/// Failures while reading or producing JSON data.
#[derive(Debug, thiserror::Error)]
pub enum SerializationError {
    #[error("could not parse JSON artifact at {path}")]
    ReadJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("could not serialize JSON artifact at {path}")]
    WriteJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("could not serialize command output")]
    Output {
        #[source]
        source: serde_json::Error,
    },
    #[error("serialization error: {message}")]
    Invalid { message: String },
}

/// The top-level error returned by the command application.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("validation failed")]
    Validation,
    #[error("blocking state: {0}")]
    Blocking(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serialization(#[from] SerializationError),
    #[error("command error: {0}")]
    Command(String),
}

impl AppError {
    /// Creates a configuration error that has no parser or I/O source.
    pub fn config(message: String) -> Self {
        Self::Config(ConfigError::Invalid { message })
    }

    /// Creates a user-correctable command-usage error.
    pub fn command(message: String) -> Self {
        Self::Command(message)
    }

    /// Creates a serialization error with no more specific source type.
    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Serialization(SerializationError::Invalid {
            message: message.into(),
        })
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Validation => 1,
            Self::Blocking(_) => 2,
            Self::Config(_) | Self::Command(_) => 3,
            Self::Io(_) | Self::Serialization(_) => 4,
        }
    }
}

pub fn project_root(root: Option<PathBuf>) -> Result<PathBuf, AppError> {
    let root = match root {
        Some(root) => root,
        None => std::env::current_dir()?,
    };
    Ok(root.canonicalize()?)
}
