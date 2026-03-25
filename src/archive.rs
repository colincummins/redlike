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

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use tempfile::{NamedTempFile, TempDir};

    use crate::archive::{ArchiveError, load};
    #[tokio::test]
    async fn missing_file_with_relative_filename_returns_new_store() {
        let file_path = PathBuf::new().join("test-archive");
        let store = load(file_path).await.unwrap();
        assert!(store.get(&b"missing-key".to_vec()).await.is_none());
    }

    #[tokio::test]
    async fn missing_filename_in_existing_dir_returns_new_store() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test-archive");
        let store = load(file_path).await.unwrap();
        assert!(store.get(&b"missing-key".to_vec()).await.is_none());
    }

    #[tokio::test]
    async fn file_with_bad_directory_path_returns_error() {
        let file_path = TempDir::new()
            .unwrap()
            .path()
            .join("does_not_exist")
            .join("test-archive");
        assert!(matches!(
            load(file_path).await,
            Err(ArchiveError::ReadFile(_))
        ));
    }

    #[tokio::test]
    async fn malformed_archive_returns_invalid_archive_error() {
        let mut bad_archive = NamedTempFile::new().unwrap();
        let bad_bytes = b"This is not an archive";
        bad_archive.write_all(bad_bytes).unwrap();
        assert!(matches!(
            load(bad_archive.path().into()).await,
            Err(ArchiveError::InvalidArchive(_))
        ))
    }
}
