# `cdd-storage` Exhaustive Remediation & Engineering Plan (`TODO_PLAN.md`)

This document outlines the exhaustive engineering plan to remediate all identified code smells, broken abstractions, security vulnerabilities, weak typing, tautological tests, coverage gaming, and CI/DevOps defects in `cdd-storage`.

## Global Mandates & Quality Gates
Every single change implemented from this plan **MUST** maintain and enforce:
- **100% Documentation Coverage**: Every file (`//!`), module (`///`), struct, enum, variant, trait, function, method, field, constant, and function argument (`# Arguments`), return value (`# Returns`), and error condition (`# Errors`) must be thoroughly documented. Enforced via `#![deny(missing_docs)]`, `#![deny(clippy::missing_docs_in_private_items)]`, and `cargo +nightly rustdoc -- -Z unstable-options --show-coverage`.
- **100% Test Coverage (Function, Line, Branch)**: No `#[cfg(not(test))]` exclusions to hide untested runtime wiring. All functions, lines, and conditional branches must be exercised by deterministic unit and integration tests.
- **Strong Typing & Zero `unwrap`/`expect`/`anyhow`**: Enforce domain-specific newtypes (`OrgId`, `RepoId`, `ArtifactVersion`, `RelativeFilePath`, `ApiKey`, `Etag`, `StoreKey`) with fallible construction (`TryFrom`) and a single comprehensive `AppError` enum powered by `derive_more`.
- **Strictest Lint Levels**: `#![deny(clippy::all, clippy::pedantic, clippy::nursery, clippy::unwrap_used, clippy::expect_used, missing_docs, clippy::missing_docs_in_private_items)]` with zero `#[allow(...)]` hacks where architectural fixes apply.
- **All Pre-commit Hooks Passing**: Clean execution of `pre-commit run --all-files` (formatting, compilation, clippy, tests, coverage shields).
- **Git Policy**: **NEVER** run `git push`.

---

## Phase 1: Repository Hygiene, Build System & Pre-commit Tooling

### 1.1 Remove Stale Backup & Patch Artifacts
- [x] Remove stale macOS `sed` backup artifact `README.md-E` from the workspace root.
- [x] Remove stale patch backup `.github/workflows/ci.yml.orig`.
- [x] Remove stale patch rejection file `.github/workflows/ci.yml.rej`.
- [x] Add `*-E`, `*.orig`, and `*.rej` to `.gitignore` to prevent future accidental commits of backup/patch files.

### 1.2 Fix `Cargo.toml` Architecture & Target Configuration
- [x] Split crate into a library target (`src/lib.rs`) and a thin binary entrypoint (`src/main.rs`) in `Cargo.toml` so `cargo +nightly rustdoc --lib -- -Z unstable-options --show-coverage` works without failing on missing `--lib` target.
- [x] Review dependencies in `Cargo.toml`: replace deprecated/collision-prone `sha1` with `sha2` (SHA-256) or `blake3` for cryptographic ETag generation, and add `subtle` (or constant-time byte comparison) for API key verification.
- [x] Ensure `Cargo.toml` specifies `[lints.rust]` and `[lints.clippy]` workspace/package-level lints so both `lib` and `bin` targets inherit identical strict lint rules automatically.

### 1.3 Fix `scripts/update_shields.sh` & `README.md` Badges
- [x] Add `![Test Coverage](https://img.shields.io/badge/coverage-100%25-success.svg)` and `![Doc Coverage](https://img.shields.io/badge/docs-100%25-success.svg)` badges to `README.md` so `scripts/update_shields.sh` actually has badge targets to update.
- [x] Fix `scripts/update_shields.sh` incompatibility with macOS/BSD `sed -i` vs GNU `sed -i` (avoid generating `README.md-E` by using portable temp file replacement or Python/perl/rust-based replacement).
- [x] Remove copy-pasted `DATABASE_URL=postgres://postgres:password@localhost/cdd` from `scripts/update_shields.sh` (`cdd-storage` is a blob storage service and does not use PostgreSQL).
- [x] Update `scripts/update_shields.sh` to check both `--lib` and `--bins` doc coverage and fail with a non-zero exit code if either test coverage or doc coverage drops below 100%.

### 1.4 Fix `.pre-commit-config.yaml` & `.github/workflows/ci.yml`
- [x] Remove `DATABASE_URL=postgres://postgres:password@localhost/cdd` from the `cargo-test` hook in `.pre-commit-config.yaml`.
- [x] Add a `cargo-clippy` hook (`cargo clippy --all-targets --all-features -- -D warnings`) to `.pre-commit-config.yaml`.
- [x] Add a `cargo-doc` hook (`RUSTDOCFLAGS="-D warnings" cargo doc --lib --no-deps --document-private-items`) to `.pre-commit-config.yaml`.
- [x] Remove unused `libpq-dev` and `libssl-dev` apt packages from `.github/workflows/ci.yml` unless required by a feature.
- [x] Fix Docker port mismatch in `alpine.Dockerfile` and `debian.Dockerfile`: align `EXPOSE 8085` (or configure `HOST=0.0.0.0` and `PORT=8085` via `ENV`) with the application's default port and bind address (`0.0.0.0` instead of loopback-only `127.0.0.1` inside containers).

---

## Phase 2: Strong Domain Typing & Path Traversal Security (`src/domain.rs` / `src/error.rs`)

### 2.1 Expand Unified `AppError` Enum (`src/error.rs`)
- [x] Retain `derive_more::{Display, Error, From}` on `AppError` (no `anyhow`).
- [x] Add specific, strongly-typed error variants to `AppError`:
  - [x] `AppError::InvalidIdentifier { field: &'static str, reason: &'static str }` -> maps to `400 Bad Request`.
  - [x] `AppError::PathTraversalAttempt` -> maps to `400 Bad Request` (or `403 Forbidden`).
  - [x] `AppError::EmptyPayload` -> maps to `400 Bad Request`.
  - [x] `AppError::ConfigurationError(String)` -> for startup/config validation errors.
  - [x] `AppError::IoError(#[error(source)] std::io::Error)` -> maps to `500 Internal Server Error`.
  - [x] `AppError::NotFound` -> maps to `404 Not Found`.
  - [x] `AppError::Unauthorized` -> maps to `401 Unauthorized`.
- [x] Document every variant, field, and method on `AppError` with `# Arguments`, `# Returns`, and `# Errors` sections as applicable.
- [x] Write unit tests verifying `Display`, `Error::source`, `ResponseError::status_code`, and `ResponseError::error_response` (including asserting the exact HTTP response body bytes, not just status code) for 100% of `AppError` variants.

### 2.2 Introduce Validated Domain Newtypes (`src/domain.rs`)
- [x] Create `OrgId(String)` newtype:
  - [x] Validate non-empty, maximum length (e.g., 128 chars), and strict ASCII alphanumeric + `-` / `_` characters (rejecting `.`, `..`, `/`, ``, `\0`).
  - [x] Implement `TryFrom<String>`, `TryFrom<&str>`, `AsRef<str>`, `Display`, and `serde::Deserialize` (via `#[serde(try_from = "String")]`).
- [x] Create `RepoId(String)` newtype:
  - [x] Validate non-empty, maximum length, and strict ASCII alphanumeric + `-` / `_` / `.` (explicitly rejecting `.` or `..` segments, `/`, ``, `\0`).
  - [x] Implement `TryFrom<String>`, `TryFrom<&str>`, `AsRef<str>`, `Display`, and `serde::Deserialize`.
- [x] Create `ArtifactVersion(String)` newtype:
  - [x] Validate non-empty, safe version characters (alphanumeric, `.`, `-`, `+`, `_`), explicitly rejecting `.`, `..`, `/`, ``, `\0`.
  - [x] Implement `TryFrom<String>`, `TryFrom<&str>`, `AsRef<str>`, `Display`, and `serde::Deserialize`.
- [x] Create `RelativeFilePath(String)` newtype:
  - [x] Validate non-empty relative path composed strictly of `std::path::Component::Normal` segments (rejecting `Component::ParentDir` (`..`), `Component::CurDir` (`.`), `Component::RootDir`, `Component::Prefix`, null bytes, and leading/trailing slashes).
  - [x] Implement `TryFrom<String>`, `TryFrom<&str>`, `AsRef<str>`, `Display`, and `serde::Deserialize`.
- [x] Create `ApiKey(String)` newtype:
  - [x] Implement constant-time equality comparison against bearer token strings to eliminate timing side-channel attacks in `upload_artifact`.
  - [x] Redact secret value in custom `fmt::Debug` implementation (`ApiKey([REDACTED])`) to prevent accidental secret leakage in logs.
  - [x] Reject empty API keys during construction.
- [x] Create `Etag(String)` newtype:
  - [x] Compute strong ETag deterministically from `Bytes` payload using SHA-256 (or documented hash) formatted as `"<hex>"`.
  - [x] Provide `matches_if_none_match(&self, header_val: &str) -> bool` supporting standard RFC 9110 `If-None-Match` syntax (including comma-separated ETag lists, `W/` weak prefix awareness, and `*`).
- [x] Create validated `StoreKey`:
  - [x] Remove raw unchecked `StoreKey::new(String)` constructor.
  - [x] Provide typed constructors:
    - [x] `StoreKey::for_versioned_artifact(org: &OrgId, repo: &RepoId, version: &ArtifactVersion, file: &RelativeFilePath) -> Self`
    - [x] `StoreKey::for_path(org: &OrgId, repo: &RepoId, file_path: &RelativeFilePath) -> Self`
    - [x] `StoreKey::try_new(raw: impl Into<String>) -> Result<Self, AppError>` that validates all path components are `Component::Normal`.
- [x] Write comprehensive unit tests for every domain type covering valid inputs, empty inputs, path traversal (`../`, `/etc/passwd`, `a/../../b`, null bytes), overlong inputs, and `Debug` redaction on `ApiKey`.

---

## Phase 3: Storage Abstraction & Unified Upload/Download Contract (`src/storage.rs`)

### 3.1 Fix the `ArtifactStore` Trait & Generic `AppState` Abstraction
- [x] Make `AppState<S: ArtifactStore = LocalDiskStore>` generic over `S: ArtifactStore` instead of hardcoding `pub store: LocalDiskStore` in `src/api.rs`.
- [x] Ensure all API handlers (`upload_artifact<S: ArtifactStore>`, `download_artifact<S: ArtifactStore>`, `delete_artifact<S: ArtifactStore>`) operate on `web::Data<AppState<S>>`, fulfilling the architecture documented in `ARCHITECTURE.md` and `README.md`.
- [x] Introduce `StoredArtifact` struct returned by `ArtifactStore::get`:
  - [x] Fields: `pub data: Bytes`, `pub modified: std::time::SystemTime`, `pub etag: Etag`.
  - [x] Eliminates anonymous tuple `(Bytes, std::time::SystemTime)` and avoids recomputing hashes unnecessarily if cached or precomputed.

### 3.2 Harden `LocalDiskStore` Implementation
- [x] Atomic writes in `LocalDiskStore::put`:
  - [x] Write payload to a temporary file in the target directory (or staging dir) and atomically rename (`fs::rename`) to the final path so concurrent readers never observe partially written or truncated files.
- [x] Safe path resolution in `LocalDiskStore::resolve_path`:
  - [x] Guarantee via `StoreKey` invariants (and defensive component iteration) that the resolved `PathBuf` is strictly contained within `self.base_dir`.
- [x] Explicit directory vs. regular file handling in `LocalDiskStore::get`:
  - [x] If `metadata.is_dir()` is true, return `Ok(None)` (or a deterministic `AppError`) rather than relying on OS-specific `fs::read` failure on a directory.
- [x] Ensure `LocalDiskStore::delete` is wired to either an authenticated `DELETE /artifact/{org_id}/{repo_id}/{file_path:.*}` endpoint or documented administrative lifecycle API so `ArtifactStore::delete` is not dead production code.

### 3.3 Reconcile Upload & Download Key Symmetry (`src/api.rs`)
- [x] Fix the asymmetric storage key bug between `upload_artifact` and `download_artifact`:
  - [x] Currently, `POST /upload/{org_id}/{repo_id}/{version}` hardcodes `{org_id}/{repo_id}/{version}/artifact.bin`, while tests/docs expect files like `{org_id}/{repo_id}/{version}/{file_path}` or `{org_id}/{repo_id}/schema.json`.
  - [x] Support uploading specific artifact files via `POST /upload/{org_id}/{repo_id}/{version}/{file_path:.*}` (and optionally default to `artifact.bin` on `/upload/{org_id}/{repo_id}/{version}` for backward compatibility) so any uploaded artifact can be directly retrieved via `GET /artifact/{org_id}/{repo_id}/{version}/{file_path}` or `GET /artifact/{org_id}/{repo_id}/{file_path}`.
- [x] Add `DELETE /artifact/{org_id}/{repo_id}/{file_path:.*}` endpoint (protected by Bearer `ApiKey` authentication) that invokes `ArtifactStore::delete`, completing CRUD lifecycle support.
- [x] Remove the `#[allow(clippy::future_not_send)]` and manual `drop(req)` smell in `upload_artifact` and `download_artifact` by extracting typed headers or helper functions cleanly before any `.await` point.
- [x] Set accurate `Content-Type` header on `download_artifact` based on file extension (e.g., `application/json` for `.json`, `application/yaml` for `.yaml`/`.yml`, `application/zip` for `.zip`, `text/html; charset=utf-8` for `.html`, defaulting to `application/octet-stream`).

---

## Phase 4: Configuration, Server Wiring & Eliminating `#[cfg(not(test))]` (`src/config.rs`, `src/lib.rs`, `src/main.rs`)

### 4.1 Strongly-Typed Configuration (`src/config.rs`)
- [x] Create `AppConfig` struct with fields:
  - [x] `pub host: std::net::IpAddr` (default `0.0.0.0` or `127.0.0.1` configurable via `HOST`).
  - [x] `pub port: u16` (parsed strictly from `PORT`, returning `AppError::ConfigurationError` on invalid non-u16 strings instead of silently falling back via `.unwrap_or(8085)`).
  - [x] `pub storage_dir: PathBuf` (from `STORAGE_DIR`).
  - [x] `pub api_key: ApiKey` (validated via `ApiKey::try_from`, rejecting empty keys).
- [x] Implement `AppConfig::from_map(vars: &std::collections::HashMap<String, String>) -> Result<Self, AppError>` and `AppConfig::from_env() -> Result<Self, AppError>` so all configuration parsing branches are 100% unit-testable without mutating global process environment variables.

### 4.2 Testable Application Builder (`src/lib.rs`)
- [x] Extract Actix `App` route configuration into `pub fn configure_app<S: ArtifactStore>(cfg: &mut web::ServiceConfig, state: web::Data<AppState<S>>)` in `src/lib.rs`.
- [x] Remove `#[cfg(not(test))]` from `main.rs` / server setup by extracting `pub fn build_server(config: AppConfig, listener: std::net::TcpListener) -> std::io::Result<actix_web::dev::Server>`.
- [x] Write integration tests that bind to an ephemeral port (`127.0.0.1:0`), construct the real server via `build_server`, verify `/health`, `/upload/...`, `/artifact/...`, and shut down cleanly—achieving true 100% line/branch/function coverage without `#[cfg(not(test))]` exclusions.

---

## Phase 5: Eliminating Tautological & Weak Tests

### 5.1 Replace Tautological & Duplicated Logic in Tests
- [x] Fix `test_download_artifact_not_modified` in `src/api.rs`:
  - [x] Instead of copy-pasting the exact SHA-1 loop from `download_artifact` into the test, first perform a `GET` request (or assert against a known pre-calculated RFC test vector constant), extract the `ETag` header returned by the response, and send a second `GET` request with `If-None-Match: <etag>` to verify the `304 Not Modified` round-trip.
- [x] Fix `test_upload_artifact_success` and `test_download_artifact_success` in `src/api.rs`:
  - [x] Write an end-to-end round-trip test that uploads a payload via `POST /upload/...` and immediately downloads that exact artifact via `GET /artifact/...`, asserting that the response body bytes, `ETag`, `Last-Modified`, `Cache-Control`, and `Content-Type` match expectations.
- [x] Fix permissive/non-deterministic assertions in `test_local_disk_store_io_errors` (`src/storage.rs`) and `test_download_artifact_io_error` (`src/api.rs`):
  - [x] Using `assert!(matches!(res, Err(AppError::IoError(_)) | Ok(None)))` and `assert!(status == INTERNAL_SERVER_ERROR || status == NOT_FOUND)` masks regressions because either branch passes.
  - [x] Create deterministic fault-injection tests (using an in-memory mock/faulting `ArtifactStore` implementation for API tests, and deterministic filesystem states like reading a directory or uncreatable paths for `LocalDiskStore`) that assert one exact expected outcome per test.
- [x] Fix `test_health_check_ok` in `src/main.rs` and `test_app_error_response_body` in `src/error.rs`:
  - [x] Read and assert the actual HTTP response body bytes (`actix_web::body::to_bytes`) in addition to the HTTP status code.
- [x] Fix robotic/tautological test docstrings (`/// Tests test upload artifact success.`):
  - [x] Rewrite all test doc comments to clearly explain the scenario, inputs, and expected behavior under test.

---

## Phase 6: Final Verification & Sign-off Checklist

- [x] Run `cargo fmt --all -- --check` and confirm zero formatting diffs.
- [x] Run `cargo clippy --all-targets --all-features -- -D warnings` and confirm zero warnings or errors.
- [x] Run `RUSTDOCFLAGS="-D warnings" cargo doc --lib --no-deps --document-private-items` and confirm zero doc warnings.
- [x] Run `cargo +nightly rustdoc --lib -- -Z unstable-options --show-coverage` (and on binary target) and verify **100.0% documentation coverage**.
- [x] Run `cargo tarpaulin --engine llvm --out Xml --out Lcov` (or `cargo llvm-cov --all-targets --branch`) and verify **100.0% function, line, and branch test coverage**.
- [x] Run `./scripts/update_shields.sh` and verify `README.md` badges reflect `100%` test coverage and `100%` doc coverage without creating backup files.
- [x] Run `pre-commit run --all-files` and verify all hooks pass.
- [x] Confirm `git status` shows no untracked temporary files (`README.md-E`, `*.orig`, `*.rej`) and **do not** run `git push`.
