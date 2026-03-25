use crate::store::RestoreError;
use crate::store::Store;
use std::error;
use std::{fmt, path::PathBuf};
use tokio::fs;

#[derive(Debug)]
pub enum ArchiveError {
    ReadFile(std::io::Error),
    InvalidArchive(RestoreError),
    WriteFile(std::io::Error),
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArchiveError::ReadFile(_) => {
                write!(f, "Unable to read archive file")
            }
            ArchiveError::WriteFile(_) => {
                write!(f, "Unable to write to archive file")
            }
            ArchiveError::InvalidArchive(_) => {
                write!(f, "Invalid archive format")
            }
        }
    }
}

impl std::error::Error for ArchiveError {}

pub async fn load(path: PathBuf) -> Result<Store, ArchiveError> {
    match fs::read(&path).await {
        Ok(contents) => Store::restore(contents.as_slice())
            .await
            .map_err(ArchiveError::InvalidArchive),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Only treat it as first-run if the parent dir is usable.
            match path.parent() {
                Some(parent) if !parent.as_os_str().is_empty() && !parent.exists() => {
                    Err(ArchiveError::ReadFile(error))
                }
                _ => Ok(Store::new()),
            }
        }
        Err(error) => Err(ArchiveError::ReadFile(error)),
    }
}
