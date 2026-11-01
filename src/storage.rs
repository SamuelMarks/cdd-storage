//! Storage backend definitions and implementations.
//!
//! This module defines the [`ArtifactStore`] trait for persisting and retrieving
//! binary artifacts, [`crate::storage::StoredArtifact`] for encapsulating retrieved payloads
//! alongside their modification timestamps and strong [`crate::domain::Etag`]s, and the
//! [`LocalDiskStore`] implementation with atomic file writes and safe path resolution.

use actix_web::web::Bytes;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use tokio::fs;

use crate::domain::{ArtifactVersion, Etag, OrgId, RepoId, StoreKey};
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

    /// Lists all published version directories for the given organization and repository.
    ///
    /// Returns an empty vector if the repository directory does not exist.
    ///
    /// # Arguments
    ///
    /// * `&self` - The storage backend instance.
    /// * `org_id` - The validated [`OrgId`].
    /// * `repo_id` - The validated [`RepoId`].
    ///
    /// # Returns
    ///
    /// Returns a future resolving to a deterministically sorted `Vec<String>` of valid version tags.
    ///
    /// # Errors
    ///
    /// Returns an [`AppError`] if reading the repository directory fails due to I/O errors.
    fn list_versions(
        &self,
        org_id: &OrgId,
        repo_id: &RepoId,
    ) -> impl std::future::Future<Output = Result<Vec<String>, AppError>> + Send;

    /// Recursively lists all relative file paths stored under a specific organization, repository, and version.
    ///
    /// Returns an empty vector if the version directory does not exist.
    ///
    /// # Arguments
    ///
    /// * `&self` - The storage backend instance.
    /// * `org_id` - The validated [`OrgId`].
    /// * `repo_id` - The validated [`RepoId`].
    /// * `version` - The validated [`ArtifactVersion`].
    ///
    /// # Returns
    ///
    /// Returns a future resolving to a deterministically sorted `Vec<String>` of `/`‐separated relative file paths.
    ///
    /// # Errors
    ///
    /// Returns an [`AppError`] if traversing the version directory fails due to I/O errors.
    fn list_version_files(
        &self,
        org_id: &OrgId,
        repo_id: &RepoId,
        version: &ArtifactVersion,
    ) -> impl std::future::Future<Output = Result<Vec<String>, AppError>> + Send;
}

/// Trait alias for [`ArtifactStore`] representing a generic artifact storage backend.
pub use ArtifactStore as StorageBackend;

/// A storage backend that persists artifacts to the local filesystem using atomic writes.
#[derive(Debug, Clone)]
pub struct LocalDiskStore {
    /// The root directory where artifacts are stored.
    base_dir: PathBuf,
}

/// Type alias for [`LocalDiskStore`] representing the local disk storage backend.
pub type LocalDiskStorage = LocalDiskStore;

/// Parses an optional triplet of numeric semver prefix numbers `(major, minor, patch)`
/// from a version tag (ignoring an optional leading `'v'` or `'V'`) for deterministic ordering.
///
/// # Arguments
///
/// * `ver` - The candidate version tag string slice.
///
/// # Returns
///
/// Returns `Some((major, minor, patch))` if the version begins with `X.Y.Z` integers, or `None` otherwise.
#[must_use]
fn parse_semver_triplet(ver: &str) -> Option<(u64, u64, u64)> {
    let stripped = ver
        .strip_prefix('v')
        .or_else(|| ver.strip_prefix('V'))
        .unwrap_or(ver);
    let core = stripped
        .split(['-', '+'])
        .next()
        .filter(|s| !s.is_empty())?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Computes a total-order comparison key `(is_semver, major, minor, patch, raw)` for a version string.
///
/// Non-semver tags (`is_semver = false`) are ordered deterministically before semver tags (`is_semver = true`),
/// and semver tags are ordered numerically by `(major, minor, patch)` with lexicographical tie-breaking.
///
/// # Arguments
///
/// * `ver` - The version string slice.
///
/// # Returns
///
/// Returns a tuple `(bool, u64, u64, u64, &str)` implementing [`Ord`].
#[must_use]
fn version_sort_key(ver: &str) -> (bool, u64, u64, u64, &str) {
    if let Some((major, minor, patch)) = parse_semver_triplet(ver) {
        (true, major, minor, patch, ver)
    } else {
        (false, 0, 0, 0, ver)
    }
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
    /// Only normal path segments are appended, preventing any path traversal even in depth.
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
        for segment in key.as_str().split('/') {
            resolved.push(segment);
        }
        resolved
    }

    /// Resolves the directory path for an organization and repository (`{base_dir}/{org_id}/{repo_id}`).
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`LocalDiskStore`] instance.
    /// * `org_id` - The validated [`OrgId`].
    /// * `repo_id` - The validated [`RepoId`].
    ///
    /// # Returns
    ///
    /// Returns the resolved repository [`PathBuf`].
    #[must_use]
    fn resolve_repo_dir(&self, org_id: &OrgId, repo_id: &RepoId) -> PathBuf {
        self.base_dir.join(org_id.as_str()).join(repo_id.as_str())
    }

    /// Resolves the directory path for a specific repository version (`{base_dir}/{org_id}/{repo_id}/{version}`).
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`LocalDiskStore`] instance.
    /// * `org_id` - The validated [`OrgId`].
    /// * `repo_id` - The validated [`RepoId`].
    /// * `version` - The validated [`ArtifactVersion`].
    ///
    /// # Returns
    ///
    /// Returns the resolved version [`PathBuf`].
    #[must_use]
    fn resolve_version_dir(
        &self,
        org_id: &OrgId,
        repo_id: &RepoId,
        version: &ArtifactVersion,
    ) -> PathBuf {
        self.resolve_repo_dir(org_id, repo_id)
            .join(version.as_str())
    }
}

impl ArtifactStore for LocalDiskStore {
    async fn put(&self, key: &StoreKey, data: Bytes) -> Result<(), AppError> {
        let file_path = self.resolve_path(key);
        let parent = file_path.parent().unwrap_or(&self.base_dir);

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

    async fn list_versions(
        &self,
        org_id: &OrgId,
        repo_id: &RepoId,
    ) -> Result<Vec<String>, AppError> {
        let repo_dir = self.resolve_repo_dir(org_id, repo_id);
        let mut read_dir = match fs::read_dir(&repo_dir).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(AppError::from(e)),
        };

        let mut versions = Vec::new();
        while let Some(entry) = read_dir.next_entry().await.map_err(AppError::from)? {
            let file_type = entry.file_type().await.map_err(AppError::from)?;
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with('.') && ArtifactVersion::try_from(name.as_str()).is_ok() {
                versions.push(name);
            }
        }

        versions.sort_by(|a, b| version_sort_key(a).cmp(&version_sort_key(b)));
        Ok(versions)
    }

    async fn list_version_files(
        &self,
        org_id: &OrgId,
        repo_id: &RepoId,
        version: &ArtifactVersion,
    ) -> Result<Vec<String>, AppError> {
        let version_dir = self.resolve_version_dir(org_id, repo_id, version);
        let root_read = match fs::read_dir(&version_dir).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(AppError::from(e)),
        };

        let mut files = Vec::new();
        let mut stack: Vec<(String, fs::ReadDir)> = vec![(String::new(), root_read)];

        while let Some((prefix, mut read_dir)) = stack.pop() {
            while let Some(entry) = read_dir.next_entry().await.map_err(AppError::from)? {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                let rel_path = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                };
                let file_type = entry.file_type().await.map_err(AppError::from)?;
                if file_type.is_dir() {
                    let sub_rd = fs::read_dir(entry.path()).await.map_err(AppError::from)?;
                    stack.push((rel_path, sub_rd));
                } else {
                    files.push(rel_path);
                }
            }
        }

        files.sort();
        Ok(files)
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

    /// Verifies semver-aware version comparison and lexicographical fallback.
    #[test]
    fn test_version_comparison_sorting() {
        let mut versions = vec![
            String::from("v1.10.0"),
            String::from("v1.2.0"),
            String::from("V0.9.1"),
            String::from("1.2.0-alpha"),
            String::from("custom-branch"),
            String::from("1.2.0.1"),
        ];
        versions.sort_by(|a, b| version_sort_key(a).cmp(&version_sort_key(b)));
        assert_eq!(
            versions,
            vec![
                "1.2.0.1",
                "custom-branch",
                "V0.9.1",
                "1.2.0-alpha",
                "v1.2.0",
                "v1.10.0",
            ]
        );
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

    /// Verifies [`LocalDiskStore::list_versions`] and [`LocalDiskStore::list_version_files`]
    /// across missing directories, hidden files, non-directory entries, and nested subdirectories.
    #[actix_web::test]
    async fn test_local_disk_store_list_versions_and_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let org = OrgId::try_from("acme")?;
        let repo = RepoId::try_from("acme-sdk")?;
        let v1 = ArtifactVersion::try_from("v1.0.0")?;
        let v2 = ArtifactVersion::try_from("v1.2.0")?;
        let v10 = ArtifactVersion::try_from("v1.10.0")?;

        // Initially non-existent repo and version return empty lists
        assert_eq!(
            store.list_versions(&org, &repo).await?,
            Vec::<String>::new()
        );
        assert_eq!(
            store.list_version_files(&org, &repo, &v1).await?,
            Vec::<String>::new()
        );

        // Populate multiple versions and nested documentation files
        store
            .put(
                &StoreKey::try_new("acme/acme-sdk/v1.10.0/index.html")?,
                Bytes::from_static(b"<html>v1.10.0</html>"),
            )
            .await?;
        store
            .put(
                &StoreKey::try_new("acme/acme-sdk/v1.2.0/index.html")?,
                Bytes::from_static(b"<html>v1.2.0</html>"),
            )
            .await?;
        store
            .put(
                &StoreKey::try_new("acme/acme-sdk/v1.0.0/index.html")?,
                Bytes::from_static(b"<html>v1.0.0</html>"),
            )
            .await?;
        store
            .put(
                &StoreKey::try_new("acme/acme-sdk/v1.0.0/docs.json")?,
                Bytes::from_static(b"{}"),
            )
            .await?;
        store
            .put(
                &StoreKey::try_new("acme/acme-sdk/v1.0.0/assets/app.js")?,
                Bytes::from_static(b"console.log(1);"),
            )
            .await?;

        // Create hidden files/directories, invalid version dirs, and stray top-level files to ensure they are ignored
        let repo_dir = tmp_dir.path().join("acme").join("acme-sdk");
        fs::write(repo_dir.join("stray-file.txt"), b"ignore").await?;
        fs::create_dir_all(repo_dir.join(".hidden-dir")).await?;
        fs::create_dir_all(repo_dir.join("bad$version")).await?;
        fs::write(repo_dir.join("v1.0.0").join(".DS_Store"), b"ignore").await?;

        let versions = store.list_versions(&org, &repo).await?;
        assert_eq!(versions, vec!["v1.0.0", "v1.2.0", "v1.10.0"]);

        let v1_files = store.list_version_files(&org, &repo, &v1).await?;
        assert_eq!(v1_files, vec!["assets/app.js", "docs.json", "index.html"]);

        let v2_files = store.list_version_files(&org, &repo, &v2).await?;
        assert_eq!(v2_files, vec!["index.html"]);

        let v10_files = store.list_version_files(&org, &repo, &v10).await?;
        assert_eq!(v10_files, vec!["index.html"]);

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
        let org = OrgId::try_from("org")?;
        let repo = RepoId::try_from("repo")?;
        let ver = ArtifactVersion::try_from("v1")?;

        // Attempting to create parent directories under a regular file deterministically fails with IoError
        let put_res = store.put(&key, data).await;
        assert!(matches!(put_res, Err(AppError::IoError(_))));

        // Attempting metadata on a child of a regular file fails with ENOTDIR (IoError) on POSIX
        let get_res = store.get(&key).await;
        assert!(matches!(get_res, Err(AppError::IoError(_))));

        // Attempting read_dir on a child of a regular file fails with ENOTDIR (IoError)
        let list_ver_res = store.list_versions(&org, &repo).await;
        assert!(matches!(list_ver_res, Err(AppError::IoError(_))));

        let list_files_res = store.list_version_files(&org, &repo, &ver).await;
        assert!(matches!(list_files_res, Err(AppError::IoError(_))));

        Ok(())
    }
}
