# `cdd-storage` Exhaustive Feature & Quality Plan (`cdd-storage_TODO_PLAN.md`)

This document defines the exhaustive, file-by-file engineering checklist for `cdd-storage` to deliver:
- **Requirement 7 (Official CDD Documentation Site Hosting & Versioned Static Bundle Delivery)**:
  - Host compiled `cdd-docs-ui` static bundles, `docs.json`, `openapi.json`, `arazzo.json`, and language-specific documentation assets (`Javadoc`, `Rustdoc`, `TypeDoc`, `Sphinx`, `DocC`, `Doxygen`, `YARD`, `DocFX`) for the **Official CDD Doc Site**.
  - Support both **Monorepo** (`${GITHUB_ORG_NAME}-sdk`) and **Polyrepo** (`${GITHUB_ORG_NAME}-sdk-${LANGUAGE}`) documentation namespaces.
  - Expose site metadata/version index endpoints (`GET /sites/{org_id}/{repo_id}` and `GET /sites/{org_id}/{repo_id}/{version}`) and serve static documentation files with accurate `Content-Type` MIME headers and `ETag` conditional caching.

---

## Global Quality Mandates (Strictly Enforced)
- [x] **100% Documentation Coverage**:
  - Every module (`//!`), struct, enum, variant, field, trait, and function (`///`) MUST have complete rustdoc documentation.
  - Every function/method MUST include `# Arguments`, `# Returns`, and (where fallible) `# Errors` sections.
  - Enforce `#![deny(missing_docs)]`, `#![deny(clippy::missing_docs_in_private_items)]`, and `#![deny(rustdoc::broken_intra_doc_links)]` in `src/lib.rs` and `src/main.rs` (via `[lints]` in `Cargo.toml`).
- [x] **100% Test Coverage (Function, Line, Branch)**:
  - Every function, line, and branch in `src/` MUST be covered by unit and integration tests.
  - Verified via `cargo llvm-cov --all-targets` and `cargo +nightly llvm-cov --branch --all-targets` (`100.00%` Functions, `100.00%` Lines, `100.00%` Branches).
- [x] **Strong Typing**:
  - Use validated newtypes (`OrgId`, `RepoId`, `ArtifactVersion` / `VersionTag`, `RelativeFilePath` / `RelativeArtifactPath`, `ApiKey`, `Etag`, `StoreKey`, `ContentMimeType`, `DocSiteIndex`, `DocSiteVersionSummary`) for all inputs and outputs.
- [x] **Zero `unwrap` / `expect` / `panic`**:
  - Enforce `#![deny(clippy::unwrap_used)]`, `#![deny(clippy::expect_used)]`, and `#![deny(clippy::panic)]` across library, binary, and test targets.
- [x] **Single Unified Error Enum with `derive_more`**:
  - No `anyhow` or `thiserror`. All errors flow through `crate::error::AppError` (`crate::error::StorageError`) using `derive_more::{Display, Error, From}`.

---

## Phase 1: Domain Types for Official Doc Site & MIME Detection (`src/domain.rs`)
- [x] Add `ContentMimeType` enum in `src/domain.rs`:
  - Variants:
    - `Html` (`text/html; charset=utf-8`)
    - `Css` (`text/css; charset=utf-8`)
    - `JavaScript` (`application/javascript; charset=utf-8`)
    - `Json` (`application/json`)
    - `Yaml` (`application/yaml`)
    - `Svg` (`image/svg+xml`)
    - `Png` (`image/png`)
    - `Wasm` (`application/wasm`)
    - `Zip` (`application/zip`)
    - `PlainText` (`text/plain; charset=utf-8`)
    - `OctetStream` (`application/octet-stream`)
  - Implement `ContentMimeType::from_path(path: &RelativeArtifactPath) -> Self` inspecting file extensions case-insensitively (`.html`, `.htm`, `.css`, `.js`, `.mjs`, `.json`, `.yaml`, `.yml`, `.svg`, `.png`, `.wasm`, `.zip`, `.txt`, `.md`).
  - Implement `ContentMimeType::as_str(self) -> &'static str`.
- [x] Add `DocSiteVersionSummary` and `DocSiteIndex` structs in `src/domain.rs` (`#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]`):
  - `org_id: String`
  - `repo_id: String`
  - `versions: Vec<String>`
  - `latest_version: Option<String>`
  - `files: Vec<String>`
- [x] Write unit tests in `src/domain.rs` covering every `ContentMimeType` extension branch and `DocSiteIndex` / `DocSiteVersionSummary` serialization.

---

## Phase 2: Storage Backend Version & File Listing (`src/storage.rs` & `src/api.rs`)

### 2.1 Storage Backend Listing (`src/storage.rs`)
- [x] Extend `ArtifactStore` (`StorageBackend`) trait and `LocalDiskStore` (`LocalDiskStorage`) in `src/storage.rs`:
  - [x] `list_versions(&self, org_id: &OrgId, repo_id: &RepoId) -> Result<Vec<String>, AppError>`
  - [x] `list_version_files(&self, org_id: &OrgId, repo_id: &RepoId, version: &ArtifactVersion) -> Result<Vec<String>, AppError>`
- [x] Ensure deterministic semver/lexicographical sorting of versions and relative file paths.
- [x] Write unit tests in `src/storage.rs` covering empty directories, multiple versions, nested files, hidden/invalid entries, and I/O errors.

### 2.2 Official Doc Site HTTP Endpoints (`src/api.rs` & `src/lib.rs`)
- [x] Add `GET /sites/{org_id}/{repo_id}` returning `DocSiteIndex` JSON listing all published documentation versions for the repository (`${GITHUB_ORG_NAME}-sdk` or `${GITHUB_ORG_NAME}-sdk-${LANGUAGE}`).
- [x] Add `GET /sites/{org_id}/{repo_id}/{version}` returning `DocSiteVersionSummary` JSON listing all files published under that specific version.
- [x] Add `GET /sites/{org_id}/{repo_id}/{version}/{file_path:.*}` serving static documentation site files with the resolved `ContentMimeType` header and `ETag` / `If-None-Match` `304 Not Modified` caching (defaulting empty `file_path` to `index.html`).
- [x] Write unit and integration tests in `src/api.rs` covering `GET /sites/...`, MIME headers, `304 Not Modified`, `400 Bad Request`, `404 Not Found`, and `500 Internal Server Error` branches (100% coverage).
