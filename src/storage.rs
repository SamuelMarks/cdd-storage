//! Storage backend definitions and implementations.

use actix_web::web::Bytes;
use std::path::PathBuf;
use tokio::fs;

use crate::error::AppError;

/// Strongly-typed key representing an artifact's unique location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreKey {
    /// The string representation of the path.
    path: String,
}

impl StoreKey {
    /// Creates a new `StoreKey`.
    ///
    /// # Arguments
    ///
    /// * `path` - The string representation of the path.
    #[must_use]
    pub const fn new(path: String) -> Self {
        Self { path }
    }

    /// Returns the path string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.path
    }
}

/// The interface for an artifact storage backend.
pub trait ArtifactStore: Send + Sync + Clone + 'static {
    /// Writes the `data` to the store at the given `key`.
    ///
    /// # Errors
    /// Returns an `AppError` if the write operation fails.
    fn put(
        &self,
        key: &StoreKey,
        data: Bytes,
    ) -> impl std::future::Future<Output = Result<(), AppError>> + Send;

    /// Retrieves the data from the store at the given `key`.
    ///
    /// # Errors
    /// Returns an `AppError` if the read operation fails due to unexpected errors.
    fn get(
        &self,
        key: &StoreKey,
    ) -> impl std::future::Future<Output = Result<Option<(Bytes, std::time::SystemTime)>, AppError>> + Send;

    /// Deletes the data from the store at the given `key`.
    ///
    /// # Errors
    /// Returns an `AppError` if the delete operation fails.
    fn delete(
        &self,
        key: &StoreKey,
    ) -> impl std::future::Future<Output = Result<(), AppError>> + Send;
}

/// A storage backend that persists artifacts to the local filesystem.
#[derive(Debug, Clone)]
pub struct LocalDiskStore {
    /// The root directory where artifacts are stored.
    base_dir: PathBuf,
}

impl LocalDiskStore {
    /// Creates a new `LocalDiskStore` rooted at `base_dir`.
    #[must_use]
    pub const fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Resolves a `StoreKey` to a full `PathBuf` within the `base_dir`.
    #[must_use]
    fn resolve_path(&self, key: &StoreKey) -> PathBuf {
        self.base_dir.join(key.as_str())
    }
}

impl ArtifactStore for LocalDiskStore {
    async fn put(&self, key: &StoreKey, data: Bytes) -> Result<(), AppError> {
        let file_path = self.resolve_path(key);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await.map_err(AppError::from)?;
        }
        fs::write(&file_path, data).await.map_err(AppError::from)?;
        Ok(())
    }

    async fn get(
        &self,
        key: &StoreKey,
    ) -> Result<Option<(Bytes, std::time::SystemTime)>, AppError> {
        let file_path = self.resolve_path(key);
        let metadata = match fs::metadata(&file_path).await {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(AppError::from(e)),
        };
        let modified = metadata
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let data = fs::read(&file_path).await.map_err(AppError::from)?;
        Ok(Some((Bytes::from(data), modified)))
    }

    async fn delete(&self, key: &StoreKey) -> Result<(), AppError> {
        let file_path = self.resolve_path(key);
        match fs::remove_file(&file_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::from(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_store_key() {
        let key = StoreKey::new(String::from("foo/bar.txt"));
        assert_eq!(key.as_str(), "foo/bar.txt");
    }

    #[tokio::test]
    async fn test_local_disk_store_put_get_delete() -> Result<(), Box<dyn std::error::Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let key = StoreKey::new(String::from("org/repo/v1/data.bin"));
        let data = Bytes::from_static(b"hello world");

        // GET initially empty
        let res = store.get(&key).await?;
        assert!(res.is_none());

        // DELETE initially empty (should not error)
        store.delete(&key).await?;

        // PUT data
        store.put(&key, data.clone()).await?;

        // GET data back
        let (retrieved_data, _) = store.get(&key).await?.ok_or("Not found")?;
        assert_eq!(retrieved_data, data);

        // DELETE data
        store.delete(&key).await?;

        // GET empty again
        let res = store.get(&key).await?;
        assert!(res.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_local_disk_store_io_errors() -> Result<(), Box<dyn std::error::Error>> {
        // Test how the store behaves with an invalid base directory.
        // Creating a store in a file path (not a directory) to force IO errors.
        let tmp_file = tempfile::NamedTempFile::new()?;
        let store = LocalDiskStore::new(tmp_file.path().to_path_buf());
        let key = StoreKey::new(String::from("org/repo/v1/data.bin"));
        let data = Bytes::from_static(b"hello world");

        // PUT should fail because base_dir is a file, so it cannot create dirs/files inside it.
        let put_res = store.put(&key, data).await;
        assert!(matches!(put_res, Err(AppError::IoError(_))));

        // GET should return None if the parent doesn't exist/is a file (NotFound or NotADirectory mapped to error)
        let get_res = store.get(&key).await;
        // In many OS, reading inside a file returns ENOTDIR which is an error, not NotFound.
        // So we expect an Err here, or possibly None if it translates to NotFound.
        assert!(matches!(get_res, Err(AppError::IoError(_)) | Ok(None)));

        // DELETE should also handle ENOTDIR or similar without panicking
        let delete_res = store.delete(&key).await;
        assert!(matches!(delete_res, Err(AppError::IoError(_)) | Ok(())));

        Ok(())
    }

    #[tokio::test]
    async fn test_local_disk_store_get_dir() -> Result<(), Box<dyn std::error::Error>> {
        let tmp_dir = tempfile::TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let key = StoreKey::new(String::from("dir"));
        tokio::fs::create_dir(tmp_dir.path().join("dir")).await?;
        let res = store.get(&key).await;
        assert!(matches!(res, Err(AppError::IoError(_))));
        Ok(())
    }

    #[tokio::test]
    async fn test_local_disk_store_no_parent() {
        let store = LocalDiskStore::new(PathBuf::new());
        let key = StoreKey::new(String::new());
        // file_path will be "", which has no parent, covering the empty parent branch.
        // The write will fail because "" is not a valid file path to write.
        let put_res = store.put(&key, Bytes::from_static(b"")).await;
        assert!(matches!(put_res, Err(AppError::IoError(_))));
    }
}
