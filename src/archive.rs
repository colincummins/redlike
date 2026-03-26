use crate::store::RestoreError;
use crate::store::Store;
use std::{fmt, path::PathBuf};
use tempfile::Builder;
use tokio::fs;
use tokio::fs::File;
use tokio::fs::rename;
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
pub enum ArchiveError {
    ReadFile(std::io::Error),
    InvalidArchive(RestoreError),
    InvalidStore(serde_json::Error),
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
            ArchiveError::InvalidStore(_) => {
                write!(f, "Unable to serialize store")
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

pub async fn save(path: PathBuf, store: Store) -> Result<(), ArchiveError> {
    let bytes = store.dump().await.map_err(ArchiveError::InvalidStore)?;
    save_bytes(&path, &bytes)
        .await
        .map_err(ArchiveError::WriteFile)?;
    Ok(())
}

async fn save_bytes(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or(std::path::Path::new("."));
    let temp_archive = Builder::new()
        .prefix("archive.")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    let path_tmp = temp_archive.path().to_path_buf();
    let mut temp_archive = File::from_std(temp_archive.into_file());

    temp_archive.write_all(bytes).await?;
    temp_archive.sync_all().await?;
    drop(temp_archive);

    rename(path_tmp, path).await?;

    #[cfg(unix)]
    {
        let dir = File::open(parent).await?;
        dir.sync_all().await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use tempfile::{NamedTempFile, TempDir};

    use crate::archive::{ArchiveError, load};
    #[tokio::test]
    async fn load_missing_file_with_relative_filename_returns_new_store() {
        let file_path = PathBuf::new().join("test-archive");
        let store = load(file_path).await.unwrap();
        assert!(store.get(&b"missing-key".to_vec()).await.is_none());
    }

    #[tokio::test]
    async fn load_missing_filename_in_existing_dir_returns_new_store() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test-archive");
        let store = load(file_path).await.unwrap();
        assert!(store.get(&b"missing-key".to_vec()).await.is_none());
    }

    #[tokio::test]
    async fn load_file_with_bad_directory_path_returns_error() {
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
    async fn load_malformed_archive_returns_invalid_archive_error() {
        let mut bad_archive = NamedTempFile::new().unwrap();
        let bad_bytes = b"This is not an archive";
        bad_archive.write_all(bad_bytes).unwrap();
        assert!(matches!(
            load(bad_archive.path().into()).await,
            Err(ArchiveError::InvalidArchive(_))
        ))
    }
}
