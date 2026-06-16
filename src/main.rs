//! Main entry point for the `cdd-storage` microservice.

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]

pub mod api;
pub mod error;
pub mod storage;

#[cfg(test)]
use actix_web::HttpResponse;
#[cfg(not(test))]
use actix_web::{App, HttpResponse, HttpServer, web};
#[cfg(not(test))]
use std::env;
#[cfg(not(test))]
use std::io;
#[cfg(not(test))]
use std::path::PathBuf;

/// Health check endpoint to verify the service is running.
///
/// # Returns
///
/// Returns an `HttpResponse` with status 200 OK and body "OK".
#[allow(clippy::unused_async)]
pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().body("OK")
}

/// The main entry point for the Actix Web server.
///
/// # Errors
///
/// Returns an `io::Error` if the server fails to bind to the address or encounters an issue while running.
#[cfg(not(test))]
#[actix_web::main]
#[allow(clippy::literal_string_with_formatting_args)]
async fn main() -> io::Result<()> {
    let port = 8080;

    // Fallback configurations for local development
    let base_dir = env::var("STORAGE_DIR").unwrap_or_else(|_| String::from("./data"));
    let api_key = env::var("API_KEY").unwrap_or_else(|_| String::from("dev-secret-key"));

    let store = storage::LocalDiskStore::new(PathBuf::from(base_dir));
    let state = api::AppState { store, api_key };

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/health", web::get().to(health_check))
            .route(
                "/upload/{org_id}/{repo_id}/{version}",
                web::post().to(api::upload_artifact),
            )
            .route(
                "/artifact/{org_id}/{repo_id}/{file_path:.*}",
                web::get().to(api::download_artifact),
            )
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_web::test]
    async fn test_health_check_ok() {
        let resp = health_check().await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    }
}
