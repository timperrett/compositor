use crate::{AppError, SerializationError};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::path::Path;
use tempfile::NamedTempFile;

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, AppError> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|source| {
        AppError::Serialization(SerializationError::ReadJson {
            path: path.to_path_buf(),
            source,
        })
    })
}

pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Io(std::io::Error::other("path has no parent")))?;
    fs::create_dir_all(parent)?;
    let text = serde_json::to_string_pretty(value).map_err(|source| {
        AppError::Serialization(SerializationError::WriteJson {
            path: path.to_path_buf(),
            source,
        })
    })? + "\n";
    let mut temporary = NamedTempFile::new_in(parent)?;
    use std::io::Write;
    temporary.write_all(text.as_bytes())?;
    temporary.flush()?;
    temporary
        .persist(path)
        .map_err(|error| AppError::Io(error.error))?;
    Ok(())
}

pub fn write_text_atomic(path: &Path, text: &str) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Io(std::io::Error::other("path has no parent")))?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    use std::io::Write;
    temporary.write_all(text.as_bytes())?;
    temporary.flush()?;
    temporary
        .persist(path)
        .map_err(|error| AppError::Io(error.error))?;
    Ok(())
}

pub fn write_text_if_changed(path: &Path, text: &str) -> Result<bool, AppError> {
    match fs::read_to_string(path) {
        Ok(existing) if existing == text => Ok(false),
        Ok(_) => {
            write_text_atomic(path, text)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_text_atomic(path, text)?;
            Ok(true)
        }
        Err(error) => Err(AppError::Io(error)),
    }
}
