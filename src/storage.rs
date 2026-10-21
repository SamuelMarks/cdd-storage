//! Storage backend definitions and implementations.
//!
//! This module defines the [`ArtifactStore`] trait for persisting and retrieving
//! binary artifacts, [`crate::storage::StoredArtifact`] for encapsulating retrieved payloads
//! alongside their modification timestamps and strong [`crate::domain::Etag`]s, and the
//! [`LocalDiskStore`] implementation with atomic file writes and safe path resolution.

use actix_web::web::Bytes;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use tokio::fs;

use crate::domain::{Etag, StoreKey};
use crate::error::AppError;

/// Monotonic counter used to disambiguate concurrent temporary staging files during atomic writes.
static ATOMIC_WRITE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Represents a stored artifact retrieved from an [`ArtifactStore`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredArtifact {
    /// The raw binary payload of the artifact.
    pub data: Bytes,
    /// The last-modified timestamp of the artifact.
    pub modified: SystemTime,
    /// The precomputed strong SHA-256 [`Etag`] of `data`.
    pub etag: Etag,
}

impl StoredArtifact {
    /// Constructs a new [`StoredArtifact`] and computes its [`Etag`] from `data`.
    ///
    /// # Arguments
    ///
    /// * `data` - The binary payload of the artifact.
    /// * `modified` - The last-modified timestamp.
    ///
    /// # Returns
    ///
    /// Returns a [`StoredArtifact`] containing `data`, `modified`, and the computed [`Etag`].
    #[must_use]
    pub fn new(data: Bytes, modified: SystemTime) -> Self {
        let etag = Etag::from_bytes(&data);
        Self {
            data,
            modified,
            etag,
        }
    }
}

/// The interface for an artifact storage backend.
pub trait ArtifactStore: Send + Sync + Clone + 'static {
    /// Writes `data` to the store at the given validated `key`.
    ///
    /// # Arguments
    ///
    /// * `&self` - The storage backend instance.
    /// * `key` - The validated [`StoreKey`] specifying the artifact location.
    /// * `data` - The binary payload to store.
    ///
    /// # Returns
    ///
    /// Returns a future resolving to `Ok(())` upon durable persistence.
    ///
    /// # Errors
    ///
    /// Returns an [`AppError`] if the write operation fails.
    fn put(
        &self,
        key: &StoreKey,
        data: Bytes,
    ) -> impl std::future::Future<Output = Result<(), AppError>> + Send;

    /// Retrieves a [`StoredArtifact`] from the store at the given validated `key`.
    ///
    /// # Arguments
    ///
    /// * `&self` - The storage backend instance.
    /// * `key` - The validated [`StoreKey`] specifying the artifact location.
    ///
    /// # Returns
    ///
    /// Returns a future resolving to `Ok(Some(StoredArtifact))` if found, or `Ok(None)` if absent.
    ///
    /// # Errors
    ///
    /// Returns an [`AppError`] if the read operation fails due to I/O errors.
    fn get(
        &self,
        key: &StoreKey,
    ) -> impl std::future::Future<Output = Result<Option<StoredArtifact>, AppError>> + Send;

    /// Deletes the artifact at the given validated `key`.
    ///
    /// Deleting a non-existent key is idempotent and succeeds with `Ok(())`.
    ///
    /// # Arguments
    ///
    /// * `&self` - The storage backend instance.
    /// * `key` - The validated [`StoreKey`] specifying the artifact location.
    ///
    /// # Returns
    ///
    /// Returns a future resolving to `Ok(())` when the artifact is removed or already absent.
    ///
    /// # Errors
    ///
    /// Returns an [`AppError`] if the delete operation fails due to I/O errors.
    fn delete(
        &self,
        key: &StoreKey,
    ) -> impl std::future::Future<Output = Result<(), AppError>> + Send;
}

/// A storage backend that persists artifacts to the local filesystem using atomic writes.
#[derive(Debug, Clone)]
pub struct LocalDiskStore {
    /// The root directory where artifacts are stored.
    base_dir: PathBuf,
}

impl LocalDiskStore {
    /// Creates a new [`LocalDiskStore`] rooted at `base_dir`.
    ///
    /// # Arguments
    ///
    /// * `base_dir` - The root directory path on the local filesystem.
    ///
    /// # Returns
    ///
    /// Returns a configured [`LocalDiskStore`].
    #[must_use]
    pub const fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Returns a reference to the configured base directory path.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`LocalDiskStore`] instance.
    ///
    /// # Returns
    ///
    /// Returns `&Path`.
    #[must_use]
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Safely resolves a validated [`StoreKey`] to a full [`PathBuf`] strictly inside `self.base_dir`.
    ///
    /// Only normal path components are appended, preventing any path traversal even in depth.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`LocalDiskStore`] instance.
    /// * `key` - The validated [`StoreKey`].
    ///
    /// # Returns
    ///
    /// Returns the resolved [`PathBuf`] within `self.base_dir`.
    #[must_use]
    fn resolve_path(&self, key: &StoreKey) -> PathBuf {
        let mut resolved = self.base_dir.clone();
        for component in Path::new(key.as_str()).components() {
            if let Component::Normal(segment) = component {
                resolved.push(segment);
            }
        }
        resolved
    }
}

impl ArtifactStore for LocalDiskStore {
    async fn put(&self, key: &StoreKey, data: Bytes) -> Result<(), AppError> {
        let file_path = self.resolve_path(key);
        let parent = file_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));

        fs::create_dir_all(parent).await.map_err(AppError::from)?;

        let seq = ATOMIC_WRITE_SEQ.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("artifact");
        let temp_path = parent.join(format!(".tmp.{pid}.{seq}.{file_name}"));

        fs::write(&temp_path, data).await.map_err(AppError::from)?;
        fs::rename(&temp_path, &file_path)
            .await
            .map_err(AppError::from)?;
        Ok(())
    }

    async fn get(&self, key: &StoreKey) -> Result<Option<StoredArtifact>, AppError> {
        let file_path = self.resolve_path(key);
        let metadata = match fs::metadata(&file_path).await {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(AppError::from(e)),
        };

        if metadata.is_dir() {
            return Ok(None);
        }

        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let raw_bytes = fs::read(&file_path).await.map_err(AppError::from)?;
        Ok(Some(StoredArtifact::new(Bytes::from(raw_bytes), modified)))
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
/// Unit tests for the storage backend.
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Verifies that [`LocalDiskStore::base_dir`] and [`StoreKey`] resolution work as expected.
    #[test]
    fn test_local_disk_store_base_dir_and_resolution() -> Result<(), AppError> {
        let base = PathBuf::from("/tmp/cdd-test-store");
        let store = LocalDiskStore::new(base.clone());
        assert_eq!(store.base_dir(), base.as_path());

        let key = StoreKey::try_new("org/repo/v1/schema.json")?;
        assert_eq!(
            store.resolve_path(&key),
            base.join("org").join("repo").join("v1").join("schema.json")
        );
        Ok(())
    }

    /// Verifies putting, getting, overwriting, and deleting artifacts in [`LocalDiskStore`].
    #[actix_web::test]
    async fn test_local_disk_store_put_get_delete() -> Result<(), Box<dyn std::error::Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let key = StoreKey::try_new("org/repo/v1/data.bin")?;
        let data = Bytes::from_static(b"hello world");

        // GET initially returns None
        let initial = store.get(&key).await?;
        assert!(initial.is_none());

        // DELETE on missing file is idempotent
        store.delete(&key).await?;

        // PUT writes atomically and persists data
        store.put(&key, data.clone()).await?;

        // GET returns StoredArtifact with matching payload and precomputed SHA-256 ETag
        let stored = store.get(&key).await?.ok_or("Expected stored artifact")?;
        assert_eq!(stored.data, data);
        assert_eq!(stored.etag, Etag::from_bytes(b"hello world"));

        // DELETE removes the file
        store.delete(&key).await?;

        // Subsequent GET returns None
        assert!(store.get(&key).await?.is_none());

        Ok(())
    }

    /// Verifies that [`LocalDiskStore::get`] returns `Ok(None)` when the target path is a directory.
    #[actix_web::test]
    async fn test_local_disk_store_get_directory_returns_none()
    -> Result<(), Box<dyn std::error::Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let key = StoreKey::try_new("org/repo/subdir")?;
        fs::create_dir_all(tmp_dir.path().join("org/repo/subdir")).await?;

        let result = store.get(&key).await?;
        assert!(result.is_none());

        // Deleting a directory via remove_file deterministically yields an IoError
        let del_res = store.delete(&key).await;
        assert!(matches!(del_res, Err(AppError::IoError(_))));

        Ok(())
    }

    /// Verifies deterministic I/O error propagation in [`LocalDiskStore`] when `base_dir` is a regular file.
    #[actix_web::test]
    async fn test_local_disk_store_deterministic_io_errors()
    -> Result<(), Box<dyn std::error::Error>> {
        let tmp_file = tempfile::NamedTempFile::new()?;
        let store = LocalDiskStore::new(tmp_file.path().to_path_buf());
        let key = StoreKey::try_new("org/repo/v1/data.bin")?;
        let data = Bytes::from_static(b"payload");

        // Attempting to create parent directories under a regular file deterministically fails with IoError
        let put_res = store.put(&key, data).await;
        assert!(matches!(put_res, Err(AppError::IoError(_))));

        // Attempting metadata on a child of a regular file fails with ENOTDIR (IoError) on POSIX
        let get_res = store.get(&key).await;
        assert!(matches!(get_res, Err(AppError::IoError(_))));

        Ok(())
    }
}
