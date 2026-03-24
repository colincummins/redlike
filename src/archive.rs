use crate::store::RestoreError;
use crate::store::Store;
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

pub async fn load(path: Option<PathBuf>) -> Result<Store, ArchiveError> {
    match path {
        Some(p) => {
            let contents = fs::read(p).await.map_err(ArchiveError::ReadFile)?;
            return Store::restore(contents.as_slice())
                .await
                .map_err(ArchiveError::InvalidArchive);
        }
        None => Ok(Store::new()),
    }
}
