pub const BUILD_VERSION: &str = env!("COMPOSITOR_BUILD_VERSION");

pub mod art;
pub mod build;
pub mod cli;
pub mod config;
pub mod diff;
pub mod discovery;
pub mod identity;
pub mod manifest;
pub mod markdown;
pub mod model;
pub mod planning;
pub mod proof;
pub mod report;
pub mod storage;
pub mod text;
pub mod validation;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("validation failed")]
    Validation,
    #[error("blocking state: {0}")]
    Blocking(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("command error: {0}")]
    Command(String),
}

impl AppError {
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
    root.unwrap_or_else(|| std::env::current_dir().expect("current directory"))
        .canonicalize()
        .map_err(AppError::Io)
}
