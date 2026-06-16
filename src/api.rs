//! API endpoints and handlers.

use actix_web::http::header;
use actix_web::{HttpRequest, HttpResponse, Responder, web};
use httpdate::fmt_http_date;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::fmt::Write;

use crate::error::AppError;
use crate::storage::{ArtifactStore, LocalDiskStore, StoreKey};

/// Application state shared across all routes.
#[derive(Clone, Debug)]
pub struct AppState {
    /// The storage backend.
    pub store: LocalDiskStore,
    /// The secret key required for internal uploads.
    pub api_key: String,
}

/// Path parameters for the upload endpoint.
#[derive(Debug, Deserialize)]
pub struct UploadParams {
    /// The organization ID.
    pub org_id: String,
    /// The repository ID.
    pub repo_id: String,
    /// The version of the artifact.
    pub version: String,
}

/// Path parameters for the download endpoint.
#[derive(Debug, Deserialize)]
pub struct DownloadParams {
    /// The organization ID.
    pub org_id: String,
    /// The repository ID.
    pub repo_id: String,
    /// The file path within the repository.
    pub file_path: String,
}

/// Handles the uploading of an artifact.
///
/// # Arguments
///
/// * `req` - The incoming HTTP request.
/// * `state` - The application state containing the store.
/// * `path` - The path parameters (`org_id`, `repo_id`, `version`).
/// * `body` - The request body payload.
///
/// # Errors
/// Returns an `AppError` if authentication fails or if the storage operation fails.
#[allow(clippy::future_not_send)]
pub async fn upload_artifact(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<UploadParams>,
    body: web::Bytes,
) -> Result<impl Responder, AppError> {
    let auth_header = req.headers().get(header::AUTHORIZATION);
    let expected_auth = format!("Bearer {}", state.api_key);
    let is_authorized = auth_header.is_some_and(|h| h.to_str().is_ok_and(|s| s == expected_auth));

    // Drop the request to ensure the async function remains `Send`
    drop(req);

    if !is_authorized {
        return Err(AppError::Unauthorized);
    }

    let key_path = format!(
        "{}/{}/{}/artifact.bin",
        path.org_id, path.repo_id, path.version
    );
    let store_key = StoreKey::new(key_path);

    state.store.put(&store_key, body).await?;

    Ok(HttpResponse::Created().body("Uploaded successfully"))
}

/// Handles downloading an artifact with caching.
///
/// # Arguments
///
/// * `req` - The incoming HTTP request.
/// * `state` - The application state containing the store.
/// * `path` - The path parameters (`org_id`, `repo_id`, `file_path`).
///
/// # Errors
/// Returns an `AppError` if the storage operation fails.
#[allow(clippy::future_not_send)]
pub async fn download_artifact(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<DownloadParams>,
) -> Result<HttpResponse, AppError> {
    let key_path = format!("{}/{}/{}", path.org_id, path.repo_id, path.file_path);
    let store_key = StoreKey::new(key_path);

    let (data, modified) = state
        .store
        .get(&store_key)
        .await?
        .ok_or(AppError::NotFound)?;

    // Generate ETag
    let mut hasher = Sha1::new();
    hasher.update(&data);
    let hash = hasher.finalize();

    let mut etag_string = String::with_capacity(42);
    etag_string.push('"');
    for b in hash {
        let _ = write!(etag_string, "{b:02x}");
    }
    etag_string.push('"');

    // Check If-None-Match
    if req
        .headers()
        .get(header::IF_NONE_MATCH)
        .is_some_and(|if_none_match| if_none_match.to_str().is_ok_and(|s| s == etag_string))
    {
        return Ok(HttpResponse::NotModified().finish());
    }

    // Format Last-Modified
    let last_modified_http = fmt_http_date(modified);

    // Return data with ETag and Last-Modified
    Ok(HttpResponse::Ok()
        .insert_header((header::ETAG, etag_string))
        .insert_header((header::LAST_MODIFIED, last_modified_http))
        .insert_header((header::CACHE_CONTROL, "public, max-age=3600"))
        .body(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, http::header, test};
    use std::error::Error;
    use tempfile::TempDir;

    #[actix_web::test]
    async fn test_upload_artifact_success() -> Result<(), Box<dyn Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let state = AppState {
            store,
            api_key: String::from("secret"),
        };

        let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
            "/upload/{org_id}/{repo_id}/{version}",
            web::post().to(upload_artifact),
        ))
        .await;

        let req = test::TestRequest::post()
            .uri("/upload/my-org/my-repo/v1")
            .insert_header((header::AUTHORIZATION, "Bearer secret"))
            .set_payload("test data")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);

        Ok(())
    }

    #[actix_web::test]
    async fn test_upload_artifact_unauthorized_missing_header() -> Result<(), Box<dyn Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let state = AppState {
            store,
            api_key: String::from("secret"),
        };

        let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
            "/upload/{org_id}/{repo_id}/{version}",
            web::post().to(upload_artifact),
        ))
        .await;

        let req = test::TestRequest::post()
            .uri("/upload/my-org/my-repo/v1")
            .set_payload("test data")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        Ok(())
    }

    #[actix_web::test]
    async fn test_upload_artifact_unauthorized_wrong_key() -> Result<(), Box<dyn Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let state = AppState {
            store,
            api_key: String::from("secret"),
        };

        let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
            "/upload/{org_id}/{repo_id}/{version}",
            web::post().to(upload_artifact),
        ))
        .await;

        let req = test::TestRequest::post()
            .uri("/upload/my-org/my-repo/v1")
            .insert_header((header::AUTHORIZATION, "Bearer wrong"))
            .set_payload("test data")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        Ok(())
    }

    #[actix_web::test]
    async fn test_upload_artifact_unauthorized_bad_header_value() -> Result<(), Box<dyn Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let state = AppState {
            store,
            api_key: String::from("secret"),
        };

        let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
            "/upload/{org_id}/{repo_id}/{version}",
            web::post().to(upload_artifact),
        ))
        .await;

        let req = test::TestRequest::post()
            .uri("/upload/my-org/my-repo/v1")
            .insert_header((header::AUTHORIZATION, b"\xff\xff\xff".as_slice())) // invalid utf-8
            .set_payload("test data")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        Ok(())
    }

    #[actix_web::test]
    async fn test_download_artifact_success() -> Result<(), Box<dyn Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let state = AppState {
            store: store.clone(),
            api_key: String::from("secret"),
        };

        let payload = web::Bytes::from_static(b"test data");
        let key = StoreKey::new(String::from("my-org/my-repo/schema.json"));
        store.put(&key, payload).await?;

        let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
            #[allow(clippy::literal_string_with_formatting_args)]
            "/artifact/{org_id}/{repo_id}/{file_path:.*}",
            web::get().to(download_artifact),
        ))
        .await;

        let req = test::TestRequest::get()
            .uri("/artifact/my-org/my-repo/schema.json")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let etag = resp
            .headers()
            .get(header::ETAG)
            .ok_or("ETag missing")?
            .to_str()?;
        let last_modified = resp
            .headers()
            .get(header::LAST_MODIFIED)
            .ok_or("Last-Modified missing")?
            .to_str()?;
        assert!(!etag.is_empty());
        assert!(!last_modified.is_empty());

        Ok(())
    }

    #[actix_web::test]
    async fn test_download_artifact_not_modified() -> Result<(), Box<dyn Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let state = AppState {
            store: store.clone(),
            api_key: String::from("secret"),
        };

        let payload = web::Bytes::from_static(b"test data");
        let key = StoreKey::new(String::from("my-org/my-repo/schema.json"));
        store.put(&key, payload).await?;

        // Compute expected ETag
        let mut hasher = Sha1::new();
        hasher.update(b"test data");
        let hash = hasher.finalize();

        let mut expected_etag = String::with_capacity(42);
        expected_etag.push('"');
        for b in hash {
            let _ = write!(expected_etag, "{b:02x}");
        }
        expected_etag.push('"');

        let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
            #[allow(clippy::literal_string_with_formatting_args)]
            "/artifact/{org_id}/{repo_id}/{file_path:.*}",
            web::get().to(download_artifact),
        ))
        .await;

        let req = test::TestRequest::get()
            .uri("/artifact/my-org/my-repo/schema.json")
            .insert_header((header::IF_NONE_MATCH, expected_etag))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_MODIFIED);

        Ok(())
    }

    #[actix_web::test]
    async fn test_download_artifact_not_found() -> Result<(), Box<dyn Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let state = AppState {
            store,
            api_key: String::from("secret"),
        };

        let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
            #[allow(clippy::literal_string_with_formatting_args)]
            "/artifact/{org_id}/{repo_id}/{file_path:.*}",
            web::get().to(download_artifact),
        ))
        .await;

        let req = test::TestRequest::get()
            .uri("/artifact/my-org/my-repo/missing.json")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);

        Ok(())
    }

    #[actix_web::test]
    async fn test_upload_artifact_io_error() -> Result<(), Box<dyn Error>> {
        let tmp_file = tempfile::NamedTempFile::new()?;
        let store = LocalDiskStore::new(tmp_file.path().to_path_buf());
        let state = AppState {
            store,
            api_key: String::from("secret"),
        };

        let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
            "/upload/{org_id}/{repo_id}/{version}",
            web::post().to(upload_artifact),
        ))
        .await;

        let req = test::TestRequest::post()
            .uri("/upload/my-org/my-repo/v1")
            .insert_header((header::AUTHORIZATION, "Bearer secret"))
            .set_payload("test data")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        Ok(())
    }

    #[actix_web::test]
    async fn test_download_artifact_io_error() -> Result<(), Box<dyn Error>> {
        let tmp_file = tempfile::NamedTempFile::new()?;
        let store = LocalDiskStore::new(tmp_file.path().to_path_buf());
        let state = AppState {
            store,
            api_key: String::from("secret"),
        };

        let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
            #[allow(clippy::literal_string_with_formatting_args)]
            "/artifact/{org_id}/{repo_id}/{file_path:.*}",
            web::get().to(download_artifact),
        ))
        .await;

        let req = test::TestRequest::get()
            .uri("/artifact/my-org/my-repo/schema.json")
            .to_request();

        let resp = test::call_service(&app, req).await;
        let status = resp.status();
        assert!(
            status == actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
                || status == actix_web::http::StatusCode::NOT_FOUND
        );

        Ok(())
    }

    #[actix_web::test]
    async fn test_download_artifact_etag_mismatch() -> Result<(), Box<dyn Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let state = AppState {
            store: store.clone(),
            api_key: String::from("secret"),
        };

        let payload = web::Bytes::from_static(b"test data");
        let key = StoreKey::new(String::from("my-org/my-repo/schema.json"));
        store.put(&key, payload).await?;

        let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
            #[allow(clippy::literal_string_with_formatting_args)]
            "/artifact/{org_id}/{repo_id}/{file_path:.*}",
            web::get().to(download_artifact),
        ))
        .await;

        let req = test::TestRequest::get()
            .uri("/artifact/my-org/my-repo/schema.json")
            .insert_header((header::IF_NONE_MATCH, "\"wrong-etag\""))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        Ok(())
    }

    #[actix_web::test]
    async fn test_download_artifact_etag_bad_header() -> Result<(), Box<dyn Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let state = AppState {
            store: store.clone(),
            api_key: String::from("secret"),
        };

        let payload = web::Bytes::from_static(b"test data");
        let key = StoreKey::new(String::from("my-org/my-repo/schema.json"));
        store.put(&key, payload).await?;

        let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
            #[allow(clippy::literal_string_with_formatting_args)]
            "/artifact/{org_id}/{repo_id}/{file_path:.*}",
            web::get().to(download_artifact),
        ))
        .await;

        let req = test::TestRequest::get()
            .uri("/artifact/my-org/my-repo/schema.json")
            .insert_header((header::IF_NONE_MATCH, b"\xff\xff\xff".as_slice())) // invalid utf-8
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        Ok(())
    }
}
