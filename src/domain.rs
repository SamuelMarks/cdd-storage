//! Validated domain types and security primitives.
//!
//! This module provides strongly-typed wrappers for organization identifiers,
//! repository identifiers, artifact versions, relative file paths, API keys,
//! cryptographic ETags, and storage keys. All types validate their invariants
//! at construction time (`TryFrom`) to prevent path traversal, injection, and
//! timing attacks.

use actix_web::web::Bytes;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::{self, Write as _};
use std::path::Path;

use crate::error::AppError;

/// Maximum allowed length in bytes for single-segment identifiers (`OrgId`, `RepoId`, `ArtifactVersion`).
const MAX_IDENTIFIER_LEN: usize = 128;

/// Maximum allowed length in bytes for a [`RelativeFilePath`].
const MAX_RELATIVE_PATH_LEN: usize = 1024;

/// Validates a single-segment identifier (`OrgId`, `RepoId`, or `ArtifactVersion`).
///
/// # Arguments
///
/// * `value` - The candidate identifier string slice.
/// * `field` - The static field name used in error reporting.
/// * `allow_dots_and_plus` - Whether `.` and `+` characters are permitted (for versions/repos).
///
/// # Returns
///
/// Returns `Ok(())` if the identifier is non-empty, within [`MAX_IDENTIFIER_LEN`],
/// not `.` or `..`, and contains only permitted ASCII characters.
///
/// # Errors
///
/// Returns [`AppError::PathTraversalAttempt`] if `value` is `.` or `..` or contains
/// path separators, or [`AppError::InvalidIdentifier`] otherwise.
fn validate_segment(
    value: &str,
    field: &'static str,
    allow_dots_and_plus: bool,
) -> Result<(), AppError> {
    if value.is_empty() {
        return Err(AppError::InvalidIdentifier {
            field,
            reason: "must not be empty",
        });
    }
    if value.len() > MAX_IDENTIFIER_LEN {
        return Err(AppError::InvalidIdentifier {
            field,
            reason: "exceeds maximum length of 128 characters",
        });
    }
    if value == "."
        || value == ".."
        || value.contains("..")
        || value.contains('/')
        || value.as_bytes().contains(&b'\\')
    {
        return Err(AppError::PathTraversalAttempt);
    }
    let valid_chars = value.bytes().all(|b| {
        b.is_ascii_alphanumeric()
            || b == b'-'
            || b == b'_'
            || (allow_dots_and_plus && (b == b'.' || b == b'+'))
    });
    if !valid_chars {
        return Err(AppError::InvalidIdentifier {
            field,
            reason: "contains disallowed characters",
        });
    }
    Ok(())
}

/// Strongly-typed, validated Organization Identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_more::Display, Deserialize)]
#[serde(try_from = "String")]
pub struct OrgId(String);

impl OrgId {
    /// Returns the inner string slice of the organization ID.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`OrgId`] reference.
    ///
    /// # Returns
    ///
    /// Returns the validated organization ID as a `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for OrgId {
    /// Returns a string slice reference to the inner organization ID.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`OrgId`] reference.
    ///
    /// # Returns
    ///
    /// Returns `&str`.
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for OrgId {
    type Error = AppError;

    /// Validates and constructs an [`OrgId`] from an owned [`String`].
    ///
    /// # Arguments
    ///
    /// * `value` - The raw organization identifier string.
    ///
    /// # Returns
    ///
    /// Returns `Ok(OrgId)` if valid.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if `value` is empty, overlong, or contains invalid characters.
    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_segment(&value, "org_id", false)?;
        Ok(Self(value))
    }
}

impl TryFrom<&str> for OrgId {
    type Error = AppError;

    /// Validates and constructs an [`OrgId`] from a string slice.
    ///
    /// # Arguments
    ///
    /// * `value` - The raw organization identifier slice.
    ///
    /// # Returns
    ///
    /// Returns `Ok(OrgId)` if valid.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if `value` is empty, overlong, or contains invalid characters.
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(String::from(value))
    }
}

/// Strongly-typed, validated Repository Identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_more::Display, Deserialize)]
#[serde(try_from = "String")]
pub struct RepoId(String);

impl RepoId {
    /// Returns the inner string slice of the repository ID.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`RepoId`] reference.
    ///
    /// # Returns
    ///
    /// Returns the validated repository ID as a `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for RepoId {
    /// Returns a string slice reference to the inner repository ID.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`RepoId`] reference.
    ///
    /// # Returns
    ///
    /// Returns `&str`.
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RepoId {
    type Error = AppError;

    /// Validates and constructs a [`RepoId`] from an owned [`String`].
    ///
    /// # Arguments
    ///
    /// * `value` - The raw repository identifier string.
    ///
    /// # Returns
    ///
    /// Returns `Ok(RepoId)` if valid.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if `value` is empty, overlong, or contains invalid/traversal characters.
    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_segment(&value, "repo_id", true)?;
        Ok(Self(value))
    }
}

impl TryFrom<&str> for RepoId {
    type Error = AppError;

    /// Validates and constructs a [`RepoId`] from a string slice.
    ///
    /// # Arguments
    ///
    /// * `value` - The raw repository identifier slice.
    ///
    /// # Returns
    ///
    /// Returns `Ok(RepoId)` if valid.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if `value` is empty, overlong, or contains invalid/traversal characters.
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(String::from(value))
    }
}

/// Strongly-typed, validated Artifact Version.
#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_more::Display, Deserialize)]
#[serde(try_from = "String")]
pub struct ArtifactVersion(String);

impl ArtifactVersion {
    /// Returns the inner string slice of the artifact version.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`ArtifactVersion`] reference.
    ///
    /// # Returns
    ///
    /// Returns the validated version string as a `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ArtifactVersion {
    /// Returns a string slice reference to the inner version.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`ArtifactVersion`] reference.
    ///
    /// # Returns
    ///
    /// Returns `&str`.
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ArtifactVersion {
    type Error = AppError;

    /// Validates and constructs an [`ArtifactVersion`] from an owned [`String`].
    ///
    /// # Arguments
    ///
    /// * `value` - The raw version string.
    ///
    /// # Returns
    ///
    /// Returns `Ok(ArtifactVersion)` if valid.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if `value` is empty, overlong, or contains invalid/traversal characters.
    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_segment(&value, "version", true)?;
        Ok(Self(value))
    }
}

impl TryFrom<&str> for ArtifactVersion {
    type Error = AppError;

    /// Validates and constructs an [`ArtifactVersion`] from a string slice.
    ///
    /// # Arguments
    ///
    /// * `value` - The raw version slice.
    ///
    /// # Returns
    ///
    /// Returns `Ok(ArtifactVersion)` if valid.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if `value` is empty, overlong, or contains invalid/traversal characters.
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(String::from(value))
    }
}

/// Type alias for [`ArtifactVersion`] representing a validated version tag.
pub type VersionTag = ArtifactVersion;

/// Strongly-typed HTTP `Content-Type` MIME classification for hosted artifacts and documentation assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, derive_more::Display)]
pub enum ContentMimeType {
    /// HTML document (`"text/html; charset=utf-8"`).
    #[display("text/html; charset=utf-8")]
    Html,
    /// CSS stylesheet (`"text/css; charset=utf-8"`).
    #[display("text/css; charset=utf-8")]
    Css,
    /// JavaScript module or script (`"application/javascript; charset=utf-8"`).
    #[display("application/javascript; charset=utf-8")]
    JavaScript,
    /// JSON document (`"application/json"`).
    #[display("application/json")]
    Json,
    /// YAML document (`"application/yaml"`).
    #[display("application/yaml")]
    Yaml,
    /// SVG vector image (`"image/svg+xml"`).
    #[display("image/svg+xml")]
    Svg,
    /// PNG raster image (`"image/png"`).
    #[display("image/png")]
    Png,
    /// WebAssembly binary module (`"application/wasm"`).
    #[display("application/wasm")]
    Wasm,
    /// ZIP archive (`"application/zip"`).
    #[display("application/zip")]
    Zip,
    /// Plain text or Markdown document (`"text/plain; charset=utf-8"`).
    #[display("text/plain; charset=utf-8")]
    PlainText,
    /// Arbitrary binary stream (`"application/octet-stream"`).
    #[display("application/octet-stream")]
    OctetStream,
}

impl ContentMimeType {
    /// Infers the [`ContentMimeType`] from the extension of a validated [`RelativeArtifactPath`].
    ///
    /// Extension matching is performed case-insensitively across `.html`, `.htm`, `.css`,
    /// `.js`, `.mjs`, `.json`, `.yaml`, `.yml`, `.svg`, `.png`, `.wasm`, `.zip`, `.txt`, and `.md`.
    ///
    /// # Arguments
    ///
    /// * `path` - The validated relative artifact path.
    ///
    /// # Returns
    ///
    /// Returns the matching [`ContentMimeType`], or [`ContentMimeType::OctetStream`] for unknown extensions.
    #[must_use]
    pub fn from_path(path: &RelativeArtifactPath) -> Self {
        let ext = Path::new(path.as_str())
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm") {
            Self::Html
        } else if ext.eq_ignore_ascii_case("css") {
            Self::Css
        } else if ext.eq_ignore_ascii_case("js") || ext.eq_ignore_ascii_case("mjs") {
            Self::JavaScript
        } else if ext.eq_ignore_ascii_case("json") {
            Self::Json
        } else if ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml") {
            Self::Yaml
        } else if ext.eq_ignore_ascii_case("svg") {
            Self::Svg
        } else if ext.eq_ignore_ascii_case("png") {
            Self::Png
        } else if ext.eq_ignore_ascii_case("wasm") {
            Self::Wasm
        } else if ext.eq_ignore_ascii_case("zip") {
            Self::Zip
        } else if ext.eq_ignore_ascii_case("txt") || ext.eq_ignore_ascii_case("md") {
            Self::PlainText
        } else {
            Self::OctetStream
        }
    }

    /// Returns the static HTTP `Content-Type` header string for this MIME type.
    ///
    /// # Arguments
    ///
    /// * `self` - The [`ContentMimeType`] variant.
    ///
    /// # Returns
    ///
    /// Returns a `&'static str` valid for an HTTP `Content-Type` header.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Html => "text/html; charset=utf-8",
            Self::Css => "text/css; charset=utf-8",
            Self::JavaScript => "application/javascript; charset=utf-8",
            Self::Json => "application/json",
            Self::Yaml => "application/yaml",
            Self::Svg => "image/svg+xml",
            Self::Png => "image/png",
            Self::Wasm => "application/wasm",
            Self::Zip => "application/zip",
            Self::PlainText => "text/plain; charset=utf-8",
            Self::OctetStream => "application/octet-stream",
        }
    }
}

/// Summary of a single published documentation site version and its hosted files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocSiteVersionSummary {
    /// Organization identifier (`${GITHUB_ORG_NAME}`).
    pub org_id: String,
    /// Repository identifier (`${GITHUB_ORG_NAME}-sdk` or `${GITHUB_ORG_NAME}-sdk-${LANGUAGE}`).
    pub repo_id: String,
    /// Published version tag.
    pub version: String,
    /// Sorted list of relative file paths hosted under this version.
    pub files: Vec<String>,
}

/// Index of all published documentation site versions and files for a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocSiteIndex {
    /// Organization identifier (`${GITHUB_ORG_NAME}`).
    pub org_id: String,
    /// Repository identifier (`${GITHUB_ORG_NAME}-sdk` or `${GITHUB_ORG_NAME}-sdk-${LANGUAGE}`).
    pub repo_id: String,
    /// Deterministically sorted list of published versions.
    pub versions: Vec<String>,
    /// The latest published version, if at least one version exists.
    pub latest_version: Option<String>,
    /// Sorted list of relative file paths for the latest (or requested) version.
    pub files: Vec<String>,
}

/// Strongly-typed, validated relative file path that guarantees zero path traversal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_more::Display, Deserialize)]
#[serde(try_from = "String")]
pub struct RelativeFilePath(String);

/// Type alias for [`RelativeFilePath`] representing a validated relative artifact path.
pub type RelativeArtifactPath = RelativeFilePath;

impl RelativeFilePath {
    /// Returns the default artifact binary filename (`"artifact.bin"`).
    ///
    /// # Returns
    ///
    /// Returns a [`RelativeFilePath`] representing `"artifact.bin"`.
    #[must_use]
    pub fn default_artifact_bin() -> Self {
        Self(String::from("artifact.bin"))
    }

    /// Returns the default documentation site index filename (`"index.html"`).
    ///
    /// # Returns
    ///
    /// Returns a [`RelativeFilePath`] representing `"index.html"`.
    #[must_use]
    pub fn default_index_html() -> Self {
        Self(String::from("index.html"))
    }

    /// Returns the inner string slice of the relative file path.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`RelativeFilePath`] reference.
    ///
    /// # Returns
    ///
    /// Returns the validated relative path as a `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Infers the strongly-typed [`ContentMimeType`] from the file extension.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`RelativeFilePath`] reference.
    ///
    /// # Returns
    ///
    /// Returns the inferred [`ContentMimeType`].
    #[must_use]
    pub fn mime_type(&self) -> ContentMimeType {
        ContentMimeType::from_path(self)
    }

    /// Infers an appropriate HTTP `Content-Type` header value from the file extension.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`RelativeFilePath`] reference.
    ///
    /// # Returns
    ///
    /// Returns a static MIME type string (e.g. `"application/json"` or `"application/octet-stream"`).
    #[must_use]
    pub fn content_type(&self) -> &'static str {
        self.mime_type().as_str()
    }
}

impl AsRef<str> for RelativeFilePath {
    /// Returns a string slice reference to the inner relative path.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`RelativeFilePath`] reference.
    ///
    /// # Returns
    ///
    /// Returns `&str`.
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Validates a relative path string ensuring all segments are normal, safe path components.
///
/// # Arguments
///
/// * `value` - The raw relative path string slice.
/// * `field` - The static field name for error reporting.
///
/// # Returns
///
/// Returns `Ok(())` if `value` is a clean relative path without traversal segments.
///
/// # Errors
///
/// Returns [`AppError::PathTraversalAttempt`] if traversal, root, or empty segments exist,
/// or [`AppError::InvalidIdentifier`] if the path is empty, overlong, or has invalid characters.
fn validate_relative_path(value: &str, field: &'static str) -> Result<(), AppError> {
    if value.is_empty() {
        return Err(AppError::InvalidIdentifier {
            field,
            reason: "must not be empty",
        });
    }
    if value.len() > MAX_RELATIVE_PATH_LEN {
        return Err(AppError::InvalidIdentifier {
            field,
            reason: "exceeds maximum length of 1024 characters",
        });
    }
    if value.starts_with('/')
        || value.ends_with('/')
        || value.contains("//")
        || value.as_bytes().contains(&92u8)
        || value.as_bytes().contains(&0u8)
        || value.split('/').any(|seg| seg == "." || seg == "..")
    {
        return Err(AppError::PathTraversalAttempt);
    }

    for seg in value.split('/') {
        validate_segment(seg, field, true)?;
    }

    Ok(())
}

impl TryFrom<String> for RelativeFilePath {
    type Error = AppError;

    /// Validates and constructs a [`RelativeFilePath`] from an owned [`String`].
    ///
    /// # Arguments
    ///
    /// * `value` - The raw relative file path string.
    ///
    /// # Returns
    ///
    /// Returns `Ok(RelativeFilePath)` if valid.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if `value` is empty, overlong, or contains traversal components.
    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_relative_path(&value, "file_path")?;
        Ok(Self(value))
    }
}

impl TryFrom<&str> for RelativeFilePath {
    type Error = AppError;

    /// Validates and constructs a [`RelativeFilePath`] from a string slice.
    ///
    /// # Arguments
    ///
    /// * `value` - The raw relative file path slice.
    ///
    /// # Returns
    ///
    /// Returns `Ok(RelativeFilePath)` if valid.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if `value` is empty, overlong, or contains traversal components.
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(String::from(value))
    }
}

/// Strongly-typed API key with redacted `Debug` formatting and constant-time verification.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    /// Verifies a raw HTTP `Authorization` header value (`"Bearer <token>"`) in constant time.
    ///
    /// # Arguments
    ///
    /// * `&self` - The configured [`ApiKey`].
    /// * `authorization_header` - The raw `Authorization` header string slice.
    ///
    /// # Returns
    ///
    /// Returns `true` if `authorization_header` starts with `"Bearer "` and the
    /// token matches this API key in constant time; returns `false` otherwise.
    #[must_use]
    pub fn verify_bearer(&self, authorization_header: &str) -> bool {
        let Some(provided) = authorization_header.strip_prefix("Bearer ") else {
            return false;
        };
        let expected_bytes = self.0.as_bytes();
        let provided_bytes = provided.as_bytes();

        if expected_bytes.len() != provided_bytes.len() {
            return false;
        }

        let mut diff = 0u8;
        for (&a, &b) in expected_bytes.iter().zip(provided_bytes.iter()) {
            diff |= a ^ b;
        }
        diff == 0
    }
}

impl fmt::Debug for ApiKey {
    /// Formats the [`ApiKey`] with its secret value redacted.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`ApiKey`] reference.
    /// * `f` - The formatter.
    ///
    /// # Returns
    ///
    /// Returns `fmt::Result`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey([REDACTED])")
    }
}

impl TryFrom<String> for ApiKey {
    type Error = AppError;

    /// Constructs a validated [`ApiKey`] from an owned [`String`].
    ///
    /// # Arguments
    ///
    /// * `value` - The raw secret key string.
    ///
    /// # Returns
    ///
    /// Returns `Ok(ApiKey)` if `value` is non-empty after trimming whitespace.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::ConfigurationError`] if the key is empty.
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(AppError::ConfigurationError(String::from(
                "API_KEY must not be empty",
            )));
        }
        Ok(Self(value))
    }
}

impl TryFrom<&str> for ApiKey {
    type Error = AppError;

    /// Constructs a validated [`ApiKey`] from a string slice.
    ///
    /// # Arguments
    ///
    /// * `value` - The raw secret key slice.
    ///
    /// # Returns
    ///
    /// Returns `Ok(ApiKey)` if `value` is non-empty.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::ConfigurationError`] if the key is empty.
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(String::from(value))
    }
}

/// Strongly-typed HTTP Entity Tag (`ETag`) computed using SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_more::Display)]
pub struct Etag(String);

impl Etag {
    /// Computes a strong SHA-256 [`Etag`] from an artifact byte payload.
    ///
    /// # Arguments
    ///
    /// * `data` - The binary payload slice.
    ///
    /// # Returns
    ///
    /// Returns a quoted strong [`Etag`] formatted as `"<64-char-sha256-hex>"`.
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let digest = hasher.finalize();

        let mut etag = String::with_capacity(66);
        etag.push('"');
        for byte in digest {
            let _ = write!(etag, "{byte:02x}");
        }
        etag.push('"');
        Self(etag)
    }

    /// Returns the formatted `ETag` header string slice (including surrounding quotes).
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`Etag`] reference.
    ///
    /// # Returns
    ///
    /// Returns `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Checks whether a client's `If-None-Match` header value matches this [`Etag`].
    ///
    /// Supports wildcard `"*"`, comma-separated lists of entity tags, and weak `W/` prefixes
    /// per RFC 9110 Section 13.1.2.
    ///
    /// # Arguments
    ///
    /// * `&self` - The current artifact's [`Etag`].
    /// * `header_value` - The raw `If-None-Match` header string slice sent by the client.
    ///
    /// # Returns
    ///
    /// Returns `true` if any tag in `header_value` matches `self`, or if `header_value` is `"*"`.
    #[must_use]
    pub fn matches_if_none_match(&self, header_value: &str) -> bool {
        let trimmed = header_value.trim();
        if trimmed == "*" {
            return true;
        }
        trimmed.split(',').any(|candidate| {
            let tag = candidate.trim();
            let normalized = tag.strip_prefix("W/").unwrap_or(tag);
            normalized == self.0
        })
    }
}

/// Strongly-typed key representing an artifact's validated relative location in the store.
#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_more::Display)]
pub struct StoreKey {
    /// The validated relative path string (guaranteed free of `..`, leading `/`, or backslashes).
    path: String,
}

impl StoreKey {
    /// Constructs a [`StoreKey`] for a versioned artifact (`{org_id}/{repo_id}/{version}/{file_path}`).
    ///
    /// # Arguments
    ///
    /// * `org_id` - The validated organization ID.
    /// * `repo_id` - The validated repository ID.
    /// * `version` - The validated artifact version.
    /// * `file_path` - The validated relative file path.
    ///
    /// # Returns
    ///
    /// Returns a guaranteed-safe [`StoreKey`].
    #[must_use]
    pub fn for_versioned_artifact(
        org_id: &OrgId,
        repo_id: &RepoId,
        version: &ArtifactVersion,
        file_path: &RelativeFilePath,
    ) -> Self {
        Self {
            path: format!("{org_id}/{repo_id}/{version}/{file_path}"),
        }
    }

    /// Constructs a [`StoreKey`] from an organization, repository, and relative file path
    /// (`{org_id}/{repo_id}/{file_path}`).
    ///
    /// # Arguments
    ///
    /// * `org_id` - The validated organization ID.
    /// * `repo_id` - The validated repository ID.
    /// * `file_path` - The validated relative file path.
    ///
    /// # Returns
    ///
    /// Returns a guaranteed-safe [`StoreKey`].
    #[must_use]
    pub fn for_path(org_id: &OrgId, repo_id: &RepoId, file_path: &RelativeFilePath) -> Self {
        Self {
            path: format!("{org_id}/{repo_id}/{file_path}"),
        }
    }

    /// Validates and constructs a [`StoreKey`] from a raw string.
    ///
    /// # Arguments
    ///
    /// * `raw` - Any string-convertible path representation.
    ///
    /// # Returns
    ///
    /// Returns `Ok(StoreKey)` if all path segments are valid normal components.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if the path is empty, overlong, or contains traversal components.
    pub fn try_new(raw: impl Into<String>) -> Result<Self, AppError> {
        let path = raw.into();
        validate_relative_path(&path, "store_key")?;
        Ok(Self { path })
    }

    /// Returns the validated relative path string slice.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`StoreKey`] reference.
    ///
    /// # Returns
    ///
    /// Returns `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.path
    }
}

/// Validates that an upload payload is non-empty.
///
/// # Arguments
///
/// * `body` - The incoming payload bytes.
///
/// # Returns
///
/// Returns `Ok(())` if `body` is not empty.
///
/// # Errors
///
/// Returns [`AppError::EmptyPayload`] if `body` is empty.
pub const fn ensure_non_empty_payload(body: &Bytes) -> Result<(), AppError> {
    if body.is_empty() {
        return Err(AppError::EmptyPayload);
    }
    Ok(())
}

#[cfg(test)]
/// Unit tests for domain types and validation primitives.
mod tests {
    use super::*;

    /// Verifies [`OrgId`] validation for valid, empty, overlong, and traversal inputs.
    #[test]
    fn test_org_id_validation() -> Result<(), AppError> {
        let org = OrgId::try_from("my_org-123")?;
        assert_eq!(org.as_str(), "my_org-123");
        assert_eq!(org.as_ref(), "my_org-123");
        assert_eq!(org.to_string(), "my_org-123");

        let org_owned = OrgId::try_from(String::from("org2"))?;
        assert_eq!(org_owned.as_str(), "org2");

        assert!(matches!(
            OrgId::try_from(""),
            Err(AppError::InvalidIdentifier {
                field: "org_id",
                ..
            })
        ));
        assert!(matches!(
            OrgId::try_from("a".repeat(129)),
            Err(AppError::InvalidIdentifier {
                field: "org_id",
                ..
            })
        ));
        assert!(matches!(
            OrgId::try_from(".."),
            Err(AppError::PathTraversalAttempt)
        ));
        assert!(matches!(
            OrgId::try_from("."),
            Err(AppError::PathTraversalAttempt)
        ));
        assert!(matches!(
            OrgId::try_from("org/sub"),
            Err(AppError::PathTraversalAttempt)
        ));
        let backslash_org = format!("org{}sub", char::from(92));
        assert!(matches!(
            OrgId::try_from(backslash_org.as_str()),
            Err(AppError::PathTraversalAttempt)
        ));
        assert!(matches!(
            OrgId::try_from("org.with.dots"),
            Err(AppError::InvalidIdentifier {
                field: "org_id",
                ..
            })
        ));
        Ok(())
    }

    /// Verifies [`RepoId`] validation for valid and invalid inputs.
    #[test]
    fn test_repo_id_validation() -> Result<(), AppError> {
        let repo = RepoId::try_from("my-repo.v1_test")?;
        assert_eq!(repo.as_str(), "my-repo.v1_test");
        assert_eq!(repo.as_ref(), "my-repo.v1_test");

        assert!(matches!(
            RepoId::try_from(""),
            Err(AppError::InvalidIdentifier {
                field: "repo_id",
                ..
            })
        ));
        assert!(matches!(
            RepoId::try_from(".."),
            Err(AppError::PathTraversalAttempt)
        ));
        assert!(matches!(
            RepoId::try_from("a..b"),
            Err(AppError::PathTraversalAttempt)
        ));
        assert!(matches!(
            RepoId::try_from("repo$bad"),
            Err(AppError::InvalidIdentifier {
                field: "repo_id",
                ..
            })
        ));
        Ok(())
    }

    /// Verifies [`ArtifactVersion`] validation for semver, hashes, and invalid inputs.
    #[test]
    fn test_artifact_version_validation() -> Result<(), AppError> {
        let ver = ArtifactVersion::try_from("1.0.0-alpha.1+build.42")?;
        assert_eq!(ver.as_str(), "1.0.0-alpha.1+build.42");
        assert_eq!(ver.as_ref(), "1.0.0-alpha.1+build.42");

        assert!(matches!(
            ArtifactVersion::try_from(""),
            Err(AppError::InvalidIdentifier {
                field: "version",
                ..
            })
        ));
        assert!(matches!(
            ArtifactVersion::try_from("../v1"),
            Err(AppError::PathTraversalAttempt)
        ));
        assert!(matches!(
            ArtifactVersion::try_from("v1/v2"),
            Err(AppError::PathTraversalAttempt)
        ));
        Ok(())
    }

    /// Verifies [`RelativeFilePath`] MIME content-type inference across all [`ContentMimeType`] variants.
    #[test]
    fn test_relative_file_path_content_mime_type() -> Result<(), AppError> {
        let default_bin = RelativeFilePath::default_artifact_bin();
        assert_eq!(default_bin.as_str(), "artifact.bin");
        assert_eq!(default_bin.as_ref(), "artifact.bin");
        assert_eq!(default_bin.mime_type(), ContentMimeType::OctetStream);
        assert_eq!(default_bin.content_type(), "application/octet-stream");
        assert_eq!(
            ContentMimeType::OctetStream.to_string(),
            "application/octet-stream"
        );

        let default_idx = RelativeFilePath::default_index_html();
        assert_eq!(default_idx.as_str(), "index.html");
        assert_eq!(default_idx.mime_type(), ContentMimeType::Html);
        assert_eq!(default_idx.content_type(), "text/html; charset=utf-8");

        let cases = [
            ("v1/openapi.json", ContentMimeType::Json, "application/json"),
            (
                "specs/schema.yaml",
                ContentMimeType::Yaml,
                "application/yaml",
            ),
            (
                "specs/schema.YML",
                ContentMimeType::Yaml,
                "application/yaml",
            ),
            (
                "docs/index.html",
                ContentMimeType::Html,
                "text/html; charset=utf-8",
            ),
            (
                "docs/index.htm",
                ContentMimeType::Html,
                "text/html; charset=utf-8",
            ),
            (
                "assets/styles.CSS",
                ContentMimeType::Css,
                "text/css; charset=utf-8",
            ),
            (
                "assets/app.js",
                ContentMimeType::JavaScript,
                "application/javascript; charset=utf-8",
            ),
            (
                "assets/module.MJS",
                ContentMimeType::JavaScript,
                "application/javascript; charset=utf-8",
            ),
            ("img/logo.svg", ContentMimeType::Svg, "image/svg+xml"),
            ("img/logo.PNG", ContentMimeType::Png, "image/png"),
            (
                "pkg/renderer.wasm",
                ContentMimeType::Wasm,
                "application/wasm",
            ),
            ("sdk/client.zip", ContentMimeType::Zip, "application/zip"),
            (
                "README.txt",
                ContentMimeType::PlainText,
                "text/plain; charset=utf-8",
            ),
            (
                "README.md",
                ContentMimeType::PlainText,
                "text/plain; charset=utf-8",
            ),
            (
                "LICENSE",
                ContentMimeType::OctetStream,
                "application/octet-stream",
            ),
        ];

        for (raw, expected_mime, expected_str) in cases {
            let path = RelativeFilePath::try_from(raw)?;
            assert_eq!(path.mime_type(), expected_mime);
            assert_eq!(path.content_type(), expected_str);
        }

        Ok(())
    }

    /// Verifies [`RelativeFilePath`] validation and path traversal rejection.
    #[test]
    fn test_relative_file_path_validation() {
        assert!(matches!(
            RelativeFilePath::try_from(""),
            Err(AppError::InvalidIdentifier {
                field: "file_path",
                ..
            })
        ));
        assert!(matches!(
            RelativeFilePath::try_from("a".repeat(1025)),
            Err(AppError::InvalidIdentifier {
                field: "file_path",
                ..
            })
        ));
        let backslash_path = format!("back{}slash", char::from(92));
        let null_path = format!("null{}byte", char::from(0));
        for bad in [
            "/etc/passwd",
            "trailing/",
            "double//slash",
            backslash_path.as_str(),
            null_path.as_str(),
            "../secret",
            "a/../../secret",
            "./file.json",
            "a/./file.json",
        ] {
            assert!(matches!(
                RelativeFilePath::try_from(bad),
                Err(AppError::PathTraversalAttempt)
            ));
        }

        assert!(matches!(
            RelativeFilePath::try_from("sub/bad$file.json"),
            Err(AppError::InvalidIdentifier {
                field: "file_path",
                ..
            })
        ));
    }

    /// Verifies serialization and deserialization of [`DocSiteVersionSummary`] and [`DocSiteIndex`].
    #[test]
    fn test_doc_site_index_and_version_summary_serde() -> Result<(), Box<dyn std::error::Error>> {
        let summary = DocSiteVersionSummary {
            org_id: String::from("acme"),
            repo_id: String::from("acme-sdk-rust"),
            version: String::from("v1.0.0"),
            files: vec![String::from("docs.json"), String::from("index.html")],
        };
        let summary_json = serde_json::to_string(&summary)?;
        let decoded_summary: DocSiteVersionSummary = serde_json::from_str(&summary_json)?;
        assert_eq!(decoded_summary, summary);

        let index = DocSiteIndex {
            org_id: String::from("acme"),
            repo_id: String::from("acme-sdk"),
            versions: vec![String::from("v0.1.0"), String::from("v1.0.0")],
            latest_version: Some(String::from("v1.0.0")),
            files: vec![
                String::from("arazzo.json"),
                String::from("docs.json"),
                String::from("index.html"),
                String::from("openapi.json"),
            ],
        };
        let index_json = serde_json::to_string(&index)?;
        let decoded_index: DocSiteIndex = serde_json::from_str(&index_json)?;
        assert_eq!(decoded_index, index);

        Ok(())
    }

    /// Verifies [`ApiKey`] construction, constant-time bearer verification, and `Debug` redaction.
    #[test]
    fn test_api_key_security() -> Result<(), AppError> {
        assert!(matches!(
            ApiKey::try_from(""),
            Err(AppError::ConfigurationError(_))
        ));
        assert!(matches!(
            ApiKey::try_from("   "),
            Err(AppError::ConfigurationError(_))
        ));

        let key = ApiKey::try_from("super-secret-token")?;
        assert_eq!(format!("{key:?}"), "ApiKey([REDACTED])");
        assert!(key.verify_bearer("Bearer super-secret-token"));
        assert!(!key.verify_bearer("Bearer wrong-secret-token"));
        assert!(!key.verify_bearer("Bearer short"));
        assert!(!key.verify_bearer("Basic super-secret-token"));
        assert!(!key.verify_bearer(""));

        Ok(())
    }

    /// Verifies [`Etag`] SHA-256 calculation and RFC 9110 `If-None-Match` evaluation.
    #[test]
    fn test_etag_calculation_and_matching() {
        // Known SHA-256 of "test data":
        // 916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9
        let etag = Etag::from_bytes(b"test data");
        let expected = r#""916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9""#;
        assert_eq!(etag.as_str(), expected);
        assert_eq!(etag.to_string(), expected);

        assert!(etag.matches_if_none_match(expected));
        assert!(etag.matches_if_none_match(&format!("W/{expected}")));
        assert!(etag.matches_if_none_match(&format!(r#""other", {expected}"#)));
        assert!(etag.matches_if_none_match("*"));
        assert!(!etag.matches_if_none_match(r#""mismatched""#));
        assert!(!etag.matches_if_none_match(""));
    }

    /// Verifies [`StoreKey`] constructors and payload emptiness check.
    #[test]
    fn test_store_key_and_payload_validation() -> Result<(), AppError> {
        let org = OrgId::try_from("acme")?;
        let repo = RepoId::try_from("billing-api")?;
        let ver = ArtifactVersion::try_from("v1.2.0")?;
        let file = RelativeFilePath::try_from("openapi.json")?;

        let versioned_key = StoreKey::for_versioned_artifact(&org, &repo, &ver, &file);
        assert_eq!(
            versioned_key.as_str(),
            "acme/billing-api/v1.2.0/openapi.json"
        );

        let path_key = StoreKey::for_path(&org, &repo, &file);
        assert_eq!(path_key.as_str(), "acme/billing-api/openapi.json");

        let parsed_key = StoreKey::try_new("acme/billing-api/v1.2.0/openapi.json")?;
        assert_eq!(parsed_key, versioned_key);

        assert!(matches!(
            StoreKey::try_new("../escape"),
            Err(AppError::PathTraversalAttempt)
        ));

        ensure_non_empty_payload(&Bytes::from_static(b"non-empty"))?;
        assert!(matches!(
            ensure_non_empty_payload(&Bytes::new()),
            Err(AppError::EmptyPayload)
        ));

        Ok(())
    }
}
