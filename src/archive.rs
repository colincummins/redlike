use crate::store::RestoreError;
use crate::store::Store;
use std::io::Write;
use std::{fmt, path::PathBuf};
use tempfile::Builder;
use tokio::fs;
use tokio::fs::File;
use tokio::fs::rename;

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
    let mut temp_archive = Builder::new()
        .prefix("archive.")
        .suffix(".tmp")
        .tempfile_in(parent)?;

    temp_archive.as_file_mut().write_all(bytes)?;
    temp_archive.as_file_mut().sync_all()?;

    rename(temp_archive.into_temp_path(), &path).await?;

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

    use tempfile::{NamedTempFile, TempDir, tempdir};
    use tokio::time::{self, Duration};

    use crate::archive::save;
    use crate::{
        archive::{ArchiveError, load},
        store::Store,
    };
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

    #[tokio::test]
    async fn round_trip_item_persistence() {
        let key = b"my_key".to_vec();
        let value = b"my_value".to_vec();
        let store = Store::new();
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("archive");
        store.set(key.clone(), value.clone()).await;
        save(path.clone(), store).await.unwrap();
        let store = load(path).await.unwrap();
        assert_eq!(store.get(&key).await.unwrap(), value);
    }

    #[tokio::test]
    async fn overwriting_erases_old_archive() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("archive");

        let key_a = b"my_key_a".to_vec();
        let value_a = b"my_value_a".to_vec();

        let key_b = b"my_key_b".to_vec();
        let value_b = b"my_value_b".to_vec();

        let store = Store::new();
        store.set(key_a.clone(), value_a.clone()).await;
        save(path.clone(), store).await.unwrap();

        let store = Store::new();
        store.set(key_b.clone(), value_b.clone()).await;
        save(path.clone(), store).await.unwrap();

        let store = load(path).await.unwrap();
        assert!(store.get(&key_a).await.is_none());
        assert_eq!(store.get(&key_b).await.unwrap(), value_b);
    }

    #[tokio::test]
    async fn save_to_missing_directory_returns_write_error() {
        let path = TempDir::new()
            .unwrap()
            .path()
            .join("does_not_exist")
            .join("archive");

        assert!(matches!(
            save(path, Store::new()).await,
            Err(ArchiveError::WriteFile(_))
        ));
    }

    #[tokio::test]
    async fn round_trip_preserves_binary_keys_and_values() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("archive");
        let store = Store::new();

        let entries = [
            (b"empty-value".to_vec(), b"".to_vec()),
            (b"non-utf-value".to_vec(), b"\xF4\xFF".to_vec()),
            (b"embedded-zero-value".to_vec(), b"hello\x00world".to_vec()),
            (b"\xF4\xFF".to_vec(), b"value".to_vec()),
        ];

        for (key, value) in &entries {
            store.set(key.clone(), value.clone()).await;
        }

        save(path.clone(), store).await.unwrap();
        let store = load(path).await.unwrap();

        for (key, value) in entries {
            assert_eq!(store.get(&key).await.unwrap(), value);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn round_trip_preserves_live_ttl_and_drops_expired_items() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("archive");
        let store = Store::new();

        let persistent_key = b"persistent-key".to_vec();
        let live_key = b"live-key".to_vec();
        let expired_key = b"expired-key".to_vec();

        store
            .set(persistent_key.clone(), b"persistent-value".to_vec())
            .await;
        store.set(live_key.clone(), b"live-value".to_vec()).await;
        store
            .set(expired_key.clone(), b"expired-value".to_vec())
            .await;

        assert_eq!(1, store.expire(live_key.clone(), 5).await);
        assert_eq!(1, store.expire(expired_key.clone(), 0).await);

        save(path.clone(), store).await.unwrap();
        let store = load(path).await.unwrap();

        assert_eq!(
            store.get(&persistent_key).await.unwrap(),
            b"persistent-value".to_vec()
        );
        assert_eq!(store.get(&live_key).await.unwrap(), b"live-value".to_vec());
        assert!(store.ttl(live_key.clone()).await > 0);
        assert!(store.get(&expired_key).await.is_none());
        assert_eq!(store.ttl(expired_key).await, -2);

        time::advance(Duration::from_secs(5)).await;
        assert!(store.get(&live_key).await.is_none());
    }

    #[tokio::test]
    async fn save_of_empty_store_loads_as_empty_store() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("archive");

        save(path.clone(), Store::new()).await.unwrap();
        let store = load(path).await.unwrap();

        assert!(store.get(&b"missing-key".to_vec()).await.is_none());
    }

    #[tokio::test]
    async fn multiple_overwrites_return_latest_contents() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("archive");

        let first = Store::new();
        first
            .set(b"first-key".to_vec(), b"first-value".to_vec())
            .await;
        save(path.clone(), first).await.unwrap();

        let second = Store::new();
        second
            .set(b"second-key".to_vec(), b"second-value".to_vec())
            .await;
        save(path.clone(), second).await.unwrap();

        let third = Store::new();
        third
            .set(b"third-key".to_vec(), b"third-value".to_vec())
            .await;
        save(path.clone(), third).await.unwrap();

        let store = load(path).await.unwrap();
        assert!(store.get(&b"first-key".to_vec()).await.is_none());
        assert!(store.get(&b"second-key".to_vec()).await.is_none());
        assert_eq!(
            store.get(&b"third-key".to_vec()).await.unwrap(),
            b"third-value".to_vec()
        );
    }
}
