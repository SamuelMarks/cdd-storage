//! Library root for the `cdd-storage` artifact persistence and retrieval microservice.
//!
//! This crate provides:
//! - [`api`]: HTTP handlers and generic application state for uploading, downloading, and deleting artifacts.
//! - [`config`]: Validated runtime configuration loading (`AppConfig`).
//! - [`domain`]: Strongly-typed domain identifiers (`OrgId`, `RepoId`, `ArtifactVersion`, `RelativeFilePath`, `ApiKey`, `Etag`, `StoreKey`).
//! - [`error`]: Unified [`error::AppError`] enum implementing `actix_web::ResponseError`.
//! - [`storage`]: Generic [`storage::ArtifactStore`] trait and atomic [`storage::LocalDiskStore`] implementation.

/// HTTP API endpoints, extractors, and handlers.
pub mod api;
/// Application configuration parsing and validation.
pub mod config;
/// Validated domain types and cryptographic ETag/key primitives.
pub mod domain;
/// Unified application error types.
pub mod error;
/// Storage backend traits and local filesystem implementation.
pub mod storage;

use actix_web::dev::Server;
use actix_web::{App, HttpServer, web};
use std::io;
use std::net::TcpListener;

use crate::api::AppState;
use crate::config::AppConfig;
use crate::error::AppError;
use crate::storage::{ArtifactStore, LocalDiskStore};

/// Configures all `cdd-storage` HTTP routes on an Actix Web [`web::ServiceConfig`].
///
/// # Arguments
///
/// * `cfg` - The Actix Web service configuration builder.
/// * `state` - The shared [`web::Data<AppState<S>>`] containing the storage backend and API key.
#[allow(clippy::literal_string_with_formatting_args)]
pub fn configure_app<S: ArtifactStore>(
    cfg: &mut web::ServiceConfig,
    state: web::Data<AppState<S>>,
) {
    cfg.app_data(state)
        .route("/health", web::get().to(api::health_check))
        .route(
            "/upload/{org_id}/{repo_id}/{version}",
            web::post().to(api::upload_artifact::<S>),
        )
        .route(
            "/upload/{org_id}/{repo_id}/{version}/{file_path:.*}",
            web::post().to(api::upload_named_artifact::<S>),
        )
        .route(
            "/artifact/{org_id}/{repo_id}/{file_path:.*}",
            web::get().to(api::download_artifact::<S>),
        )
        .route(
            "/artifact/{org_id}/{repo_id}/{file_path:.*}",
            web::delete().to(api::delete_artifact::<S>),
        )
        .route(
            "/sites/{org_id}/{repo_id}",
            web::get().to(api::get_doc_site_index::<S>),
        )
        .route(
            "/sites/{org_id}/{repo_id}/{version}",
            web::get().to(api::get_doc_site_version::<S>),
        )
        .route(
            "/sites/{org_id}/{repo_id}/{version}/{file_path:.*}",
            web::get().to(api::get_doc_site_file::<S>),
        );
}

/// Builds an Actix Web [`Server`] bound to the provided [`TcpListener`] using [`AppConfig`].
///
/// # Arguments
///
/// * `config` - The validated [`AppConfig`] specifying the storage directory and API key.
/// * `listener` - A pre-bound [`TcpListener`] for accepting incoming HTTP connections.
///
/// # Returns
///
/// Returns `Ok(Server)` ready to be awaited or spawned.
///
/// # Errors
///
/// Returns an [`io::Error`] if the HTTP server fails to bind to `listener`.
pub fn build_server(config: AppConfig, listener: TcpListener) -> io::Result<Server> {
    let store = LocalDiskStore::new(config.storage_dir);
    let state = web::Data::new(AppState::new(store, config.api_key));

    let server = HttpServer::new(move || {
        let app_state = state.clone();
        App::new().configure(move |cfg| configure_app(cfg, app_state))
    })
    .listen(listener)?
    .run();

    Ok(server)
}

/// Starts and runs the server from a map of environment variables, binding to `HOST:PORT`.
///
/// When `stop_immediately` is `true`, the server is cleanly stopped right after initialization
/// so startup and binding logic can be deterministically verified in unit tests.
///
/// # Arguments
///
/// * `vars` - Map of environment variable key-value pairs.
/// * `stop_immediately` - Whether to initiate a graceful stop immediately after starting.
///
/// # Returns
///
/// Returns `Ok(())` when the server exits cleanly.
///
/// # Errors
///
/// Returns an [`AppError`] if configuration parsing fails, or [`AppError::IoError`] if
/// binding or running the server fails.
pub async fn run_with_vars<S: std::hash::BuildHasher + Sync>(
    vars: &std::collections::HashMap<String, String, S>,
    stop_immediately: bool,
) -> Result<(), AppError> {
    let config = AppConfig::from_map(vars)?;
    let listener = TcpListener::bind((config.host, config.port)).map_err(AppError::from)?;
    let server = build_server(config, listener).map_err(AppError::from)?;
    let handle = server.handle();

    if stop_immediately {
        actix_web::rt::spawn(async move {
            handle.stop(true).await;
        });
    }

    server.await.map_err(AppError::from)
}

#[cfg(test)]
/// Integration tests for application wiring and server lifecycle.
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    /// Verifies that [`build_server`] and [`run_with_vars`] bind to an ephemeral port,
    /// initialize the Actix service factory, and shut down cleanly.
    #[actix_web::test]
    async fn test_build_server_and_run_with_vars() -> Result<(), Box<dyn std::error::Error>> {
        let tmp_dir = TempDir::new()?;
        let mut vars = HashMap::new();
        vars.insert(String::from("HOST"), String::from("127.0.0.1"));
        vars.insert(String::from("PORT"), String::from("0"));
        vars.insert(
            String::from("STORAGE_DIR"),
            tmp_dir.path().to_string_lossy().into_owned(),
        );
        vars.insert(String::from("API_KEY"), String::from("test-key"));

        run_with_vars(&vars, true).await?;

        // Also exercise the `stop_immediately == false` branch by aborting the spawned server task
        // once it has bound and entered `server.await`.
        let vars_clone = vars.clone();
        let join_handle =
            actix_web::rt::spawn(async move { run_with_vars(&vars_clone, false).await });
        actix_web::rt::task::yield_now().await;
        join_handle.abort();

        Ok(())
    }

    /// Verifies that [`run_with_vars`] propagates configuration errors deterministically.
    #[actix_web::test]
    async fn test_run_with_vars_invalid_config() {
        let mut vars = HashMap::new();
        vars.insert(String::from("PORT"), String::from("invalid-port"));

        let res = run_with_vars(&vars, true).await;
        assert!(matches!(res, Err(AppError::ConfigurationError(_))));
    }
}
