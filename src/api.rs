//! HTTP API endpoints, request extractors, and handlers.
//!
//! This module implements the HTTP interface for `cdd-storage`:
//! - `GET /health`: Liveness and readiness check.
//! - `POST /upload/{org_id}/{repo_id}/{version}`: Uploads the default `artifact.bin` for a version.
//! - `POST /upload/{org_id}/{repo_id}/{version}/{file_path:.*}`: Uploads a specific file under a version.
//! - `GET /artifact/{org_id}/{repo_id}/{file_path:.*}`: Downloads an artifact with `ETag`, `Last-Modified`, `Cache-Control`, and `Content-Type` headers, supporting `304 Not Modified` via `If-None-Match`.
//! - `DELETE /artifact/{org_id}/{repo_id}/{file_path:.*}`: Deletes a stored artifact (requires Bearer authentication).

use actix_web::http::{StatusCode, header};
use actix_web::{FromRequest, HttpRequest, HttpResponse, Responder, dev::Payload, web};
use httpdate::fmt_http_date;
use serde::Deserialize;
use std::future::{Ready, ready};

use crate::domain::{
    ApiKey, ArtifactVersion, OrgId, RelativeFilePath, RepoId, StoreKey, ensure_non_empty_payload,
};
use crate::error::AppError;
use crate::storage::{ArtifactStore, LocalDiskStore};

/// Shared application state parameterized over the [`ArtifactStore`] backend `S`.
#[derive(Clone, Debug)]
pub struct AppState<S: ArtifactStore = LocalDiskStore> {
    /// The underlying artifact storage backend.
    pub store: S,
    /// The validated secret API key required for authenticated mutations.
    pub api_key: ApiKey,
}

impl<S: ArtifactStore> AppState<S> {
    /// Constructs a new [`AppState`] with the given storage backend and [`ApiKey`].
    ///
    /// # Arguments
    ///
    /// * `store` - The storage backend implementation.
    /// * `api_key` - The validated [`ApiKey`].
    ///
    /// # Returns
    ///
    /// Returns a new [`AppState<S>`].
    #[must_use]
    pub const fn new(store: S, api_key: ApiKey) -> Self {
        Self { store, api_key }
    }
}

/// Strongly-typed path parameters for `POST /upload/{org_id}/{repo_id}/{version}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UploadVersionParams {
    /// Validated organization identifier.
    pub org_id: OrgId,
    /// Validated repository identifier.
    pub repo_id: RepoId,
    /// Validated artifact version.
    pub version: ArtifactVersion,
}

/// Strongly-typed path parameters for `POST /upload/{org_id}/{repo_id}/{version}/{file_path:.*}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UploadFileParams {
    /// Validated organization identifier.
    pub org_id: OrgId,
    /// Validated repository identifier.
    pub repo_id: RepoId,
    /// Validated artifact version.
    pub version: ArtifactVersion,
    /// Validated relative file path within the versioned release.
    pub file_path: RelativeFilePath,
}

/// Strongly-typed path parameters for `GET` and `DELETE` on `/artifact/{org_id}/{repo_id}/{file_path:.*}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ArtifactPathParams {
    /// Validated organization identifier.
    pub org_id: OrgId,
    /// Validated repository identifier.
    pub repo_id: RepoId,
    /// Validated relative file path within the repository (e.g., `v1/artifact.bin` or `schema.json`).
    pub file_path: RelativeFilePath,
}

/// Typed Actix Web extractor for the `Authorization` header that is `Send`-safe across `.await` points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationHeader(Option<String>);

impl AuthorizationHeader {
    /// Verifies that the extracted `Authorization` header matches `expected_key` in constant time.
    ///
    /// # Arguments
    ///
    /// * `&self` - The extracted [`AuthorizationHeader`].
    /// * `expected_key` - The configured [`ApiKey`] to verify against.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the header carries a valid Bearer token matching `expected_key`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Unauthorized`] if the header is missing, malformed, or does not match.
    pub fn authorize(&self, expected_key: &ApiKey) -> Result<(), AppError> {
        let is_authorized = self
            .0
            .as_deref()
            .is_some_and(|s| expected_key.verify_bearer(s));
        if !is_authorized {
            return Err(AppError::Unauthorized);
        }
        Ok(())
    }
}

impl FromRequest for AuthorizationHeader {
    type Error = AppError;
    type Future = Ready<Result<Self, Self::Error>>;

    /// Extracts the optional UTF-8 `Authorization` header from the incoming [`HttpRequest`].
    ///
    /// # Arguments
    ///
    /// * `req` - The incoming HTTP request.
    /// * `_payload` - The request body payload stream (unused).
    ///
    /// # Returns
    ///
    /// Returns an immediately ready future resolving to `Ok(AuthorizationHeader)`.
    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let header_str = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .map(String::from);
        ready(Ok(Self(header_str)))
    }
}

/// Typed Actix Web extractor for the `If-None-Match` header that is `Send`-safe across `.await` points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfNoneMatchHeader(Option<String>);

impl IfNoneMatchHeader {
    /// Returns the extracted `If-None-Match` header string slice, if present.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`IfNoneMatchHeader`] instance.
    ///
    /// # Returns
    ///
    /// Returns `Option<&str>`.
    #[must_use]
    pub fn as_deref(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

impl FromRequest for IfNoneMatchHeader {
    type Error = AppError;
    type Future = Ready<Result<Self, Self::Error>>;

    /// Extracts the optional UTF-8 `If-None-Match` header from the incoming [`HttpRequest`].
    ///
    /// # Arguments
    ///
    /// * `req` - The incoming HTTP request.
    /// * `_payload` - The request body payload stream (unused).
    ///
    /// # Returns
    ///
    /// Returns an immediately ready future resolving to `Ok(IfNoneMatchHeader)`.
    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let header_str = req
            .headers()
            .get(header::IF_NONE_MATCH)
            .and_then(|h| h.to_str().ok())
            .map(String::from);
        ready(Ok(Self(header_str)))
    }
}

/// Health check endpoint to verify that the service is running.
///
/// # Returns
///
/// Returns a `Send`-safe [`Responder`] with status `200 OK` and plain-text body `"OK"`.
pub async fn health_check() -> impl Responder {
    ready(("OK", StatusCode::OK)).await
}

/// Handles uploading the default versioned artifact (`{org_id}/{repo_id}/{version}/artifact.bin`).
///
/// # Arguments
///
/// * `auth` - The extracted [`AuthorizationHeader`].
/// * `state` - Shared [`AppState`] containing the storage backend and API key.
/// * `path` - Validated [`UploadVersionParams`] (`org_id`, `repo_id`, `version`).
/// * `body` - The non-empty binary artifact payload.
///
/// # Returns
///
/// Returns `Ok(HttpResponse)` with status `201 Created` on success.
///
/// # Errors
///
/// Returns [`AppError::Unauthorized`] if authentication fails, [`AppError::EmptyPayload`]
/// if `body` is empty, or [`AppError::IoError`] if persistence fails.
pub async fn upload_artifact<S: ArtifactStore>(
    auth: AuthorizationHeader,
    state: web::Data<AppState<S>>,
    path: web::Path<UploadVersionParams>,
    body: web::Bytes,
) -> Result<HttpResponse, AppError> {
    auth.authorize(&state.api_key)?;
    ensure_non_empty_payload(&body)?;

    let params = path.into_inner();
    let default_file = RelativeFilePath::default_artifact_bin();
    let store_key = StoreKey::for_versioned_artifact(
        &params.org_id,
        &params.repo_id,
        &params.version,
        &default_file,
    );

    state.store.put(&store_key, body).await?;
    Ok(HttpResponse::Created().body("Uploaded successfully"))
}

/// Handles uploading a named file under a version (`{org_id}/{repo_id}/{version}/{file_path}`).
///
/// # Arguments
///
/// * `auth` - The extracted [`AuthorizationHeader`].
/// * `state` - Shared [`AppState`] containing the storage backend and API key.
/// * `path` - Validated [`UploadFileParams`] (`org_id`, `repo_id`, `version`, `file_path`).
/// * `body` - The non-empty binary artifact payload.
///
/// # Returns
///
/// Returns `Ok(HttpResponse)` with status `201 Created` on success.
///
/// # Errors
///
/// Returns [`AppError::Unauthorized`] if authentication fails, [`AppError::EmptyPayload`]
/// if `body` is empty, or [`AppError::IoError`] if persistence fails.
pub async fn upload_named_artifact<S: ArtifactStore>(
    auth: AuthorizationHeader,
    state: web::Data<AppState<S>>,
    path: web::Path<UploadFileParams>,
    body: web::Bytes,
) -> Result<HttpResponse, AppError> {
    auth.authorize(&state.api_key)?;
    ensure_non_empty_payload(&body)?;

    let params = path.into_inner();
    let store_key = StoreKey::for_versioned_artifact(
        &params.org_id,
        &params.repo_id,
        &params.version,
        &params.file_path,
    );

    state.store.put(&store_key, body).await?;
    Ok(HttpResponse::Created().body("Uploaded successfully"))
}

/// Handles downloading a stored artifact with `ETag`, `Last-Modified`, `Cache-Control`, and `Content-Type` headers.
///
/// # Arguments
///
/// * `if_none_match` - The extracted [`IfNoneMatchHeader`].
/// * `state` - Shared [`AppState`] containing the storage backend.
/// * `path` - Validated [`ArtifactPathParams`] (`org_id`, `repo_id`, `file_path`).
///
/// # Returns
///
/// Returns `200 OK` with the artifact payload and caching headers, or `304 Not Modified`
/// if the client's `If-None-Match` header matches the artifact's SHA-256 [`crate::domain::Etag`].
///
/// # Errors
///
/// Returns [`AppError::NotFound`] if the artifact does not exist, or [`AppError::IoError`]
/// if reading from the store fails.
pub async fn download_artifact<S: ArtifactStore>(
    if_none_match: IfNoneMatchHeader,
    state: web::Data<AppState<S>>,
    path: web::Path<ArtifactPathParams>,
) -> Result<HttpResponse, AppError> {
    let params = path.into_inner();
    let content_type = params.file_path.content_type();
    let store_key = StoreKey::for_path(&params.org_id, &params.repo_id, &params.file_path);

    let artifact = state
        .store
        .get(&store_key)
        .await?
        .ok_or(AppError::NotFound)?;

    if if_none_match
        .as_deref()
        .is_some_and(|header_val| artifact.etag.matches_if_none_match(header_val))
    {
        return Ok(HttpResponse::NotModified()
            .insert_header((header::ETAG, artifact.etag.as_str()))
            .finish());
    }

    let last_modified_http = fmt_http_date(artifact.modified);

    Ok(HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, content_type))
        .insert_header((header::ETAG, artifact.etag.as_str()))
        .insert_header((header::LAST_MODIFIED, last_modified_http))
        .insert_header((header::CACHE_CONTROL, "public, max-age=3600"))
        .body(artifact.data))
}

/// Handles deleting a stored artifact (`DELETE /artifact/{org_id}/{repo_id}/{file_path:.*}`).
///
/// # Arguments
///
/// * `auth` - The extracted [`AuthorizationHeader`].
/// * `state` - Shared [`AppState`] containing the storage backend and API key.
/// * `path` - Validated [`ArtifactPathParams`] (`org_id`, `repo_id`, `file_path`).
///
/// # Returns
///
/// Returns `Ok(HttpResponse)` with status `204 No Content` when the artifact is deleted.
///
/// # Errors
///
/// Returns [`AppError::Unauthorized`] if authentication fails, or [`AppError::IoError`]
/// if the storage backend fails to delete the artifact.
pub async fn delete_artifact<S: ArtifactStore>(
    auth: AuthorizationHeader,
    state: web::Data<AppState<S>>,
    path: web::Path<ArtifactPathParams>,
) -> Result<HttpResponse, AppError> {
    auth.authorize(&state.api_key)?;
    let params = path.into_inner();
    let store_key = StoreKey::for_path(&params.org_id, &params.repo_id, &params.file_path);

    state.store.delete(&store_key).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
/// Unit and integration tests for the HTTP API handlers.
mod tests {
    use super::*;
    use crate::configure_app;
    use crate::storage::StoredArtifact;
    use actix_web::{App, body::to_bytes, http::StatusCode, test};
    use std::error::Error;
    use std::io;
    use tempfile::TempDir;

    /// Deterministic mock [`ArtifactStore`] that always fails with [`AppError::IoError`]
    /// to test 500 Internal Server Error branches without OS-dependent filesystem behavior.
    #[derive(Clone, Debug)]
    struct FailingStore;

    impl ArtifactStore for FailingStore {
        async fn put(&self, _key: &StoreKey, _data: web::Bytes) -> Result<(), AppError> {
            std::future::ready(Err(AppError::from(io::Error::other(
                "simulated put failure",
            ))))
            .await
        }

        async fn get(&self, _key: &StoreKey) -> Result<Option<StoredArtifact>, AppError> {
            std::future::ready(Err(AppError::from(io::Error::other(
                "simulated get failure",
            ))))
            .await
        }

        async fn delete(&self, _key: &StoreKey) -> Result<(), AppError> {
            std::future::ready(Err(AppError::from(io::Error::other(
                "simulated delete failure",
            ))))
            .await
        }
    }

    /// Verifies that `GET /health` returns `200 OK` and body `"OK"`.
    #[actix_web::test]
    async fn test_health_check_status_and_body() -> Result<(), Box<dyn Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let state = web::Data::new(AppState::new(store, ApiKey::try_from("secret")?));
        let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, state))).await;

        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body()).await?;
        assert_eq!(body.as_ref(), b"OK");
        Ok(())
    }

    /// Verifies a full upload -> download -> conditional 304 -> delete -> 404 lifecycle
    /// for both default `artifact.bin` uploads and named file uploads (`openapi.json`).
    #[actix_web::test]
    async fn test_end_to_end_upload_download_cache_and_delete_lifecycle()
    -> Result<(), Box<dyn Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let state = web::Data::new(AppState::new(store, ApiKey::try_from("secret")?));

        let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, state))).await;

        // 1. Upload default versioned artifact.bin via POST /upload/{org}/{repo}/{version}
        let upload_default_req = test::TestRequest::post()
            .uri("/upload/my-org/my-repo/v1.0.0")
            .insert_header((header::AUTHORIZATION, "Bearer secret"))
            .set_payload("binary-sdk-payload")
            .to_request();
        let upload_default_resp = test::call_service(&app, upload_default_req).await;
        assert_eq!(upload_default_resp.status(), StatusCode::CREATED);
        let upload_body = to_bytes(upload_default_resp.into_body()).await?;
        assert_eq!(upload_body.as_ref(), b"Uploaded successfully");

        // 2. Download the uploaded artifact.bin via GET /artifact/my-org/my-repo/v1.0.0/artifact.bin
        let get_default_req = test::TestRequest::get()
            .uri("/artifact/my-org/my-repo/v1.0.0/artifact.bin")
            .to_request();
        let get_default_resp = test::call_service(&app, get_default_req).await;
        assert_eq!(get_default_resp.status(), StatusCode::OK);
        assert_eq!(
            get_default_resp
                .headers()
                .get(header::CONTENT_TYPE)
                .ok_or("missing Content-Type")?
                .to_str()?,
            "application/octet-stream"
        );
        let downloaded_bin = to_bytes(get_default_resp.into_body()).await?;
        assert_eq!(downloaded_bin.as_ref(), b"binary-sdk-payload");

        // 3. Upload a named schema file via POST /upload/my-org/my-repo/v1.0.0/specs/openapi.json
        let schema_payload = r#"{"openapi":"3.1.0"}"#;
        let upload_named_req = test::TestRequest::post()
            .uri("/upload/my-org/my-repo/v1.0.0/specs/openapi.json")
            .insert_header((header::AUTHORIZATION, "Bearer secret"))
            .set_payload(schema_payload)
            .to_request();
        let upload_named_resp = test::call_service(&app, upload_named_req).await;
        assert_eq!(upload_named_resp.status(), StatusCode::CREATED);

        // 4. Download the named schema file and inspect headers & body
        let get_named_req = test::TestRequest::get()
            .uri("/artifact/my-org/my-repo/v1.0.0/specs/openapi.json")
            .to_request();
        let get_named_resp = test::call_service(&app, get_named_req).await;
        assert_eq!(get_named_resp.status(), StatusCode::OK);
        assert_eq!(
            get_named_resp
                .headers()
                .get(header::CONTENT_TYPE)
                .ok_or("missing Content-Type")?
                .to_str()?,
            "application/json"
        );
        assert_eq!(
            get_named_resp
                .headers()
                .get(header::CACHE_CONTROL)
                .ok_or("missing Cache-Control")?
                .to_str()?,
            "public, max-age=3600"
        );
        let returned_etag = get_named_resp
            .headers()
            .get(header::ETAG)
            .ok_or("missing ETag")?
            .to_str()?
            .to_owned();
        let last_modified = get_named_resp
            .headers()
            .get(header::LAST_MODIFIED)
            .ok_or("missing Last-Modified")?
            .to_str()?
            .to_owned();
        assert_ne!(returned_etag, "");
        assert_ne!(last_modified, "");

        let downloaded_schema = to_bytes(get_named_resp.into_body()).await?;
        assert_eq!(downloaded_schema.as_ref(), schema_payload.as_bytes());

        // 5. Re-request with matching If-None-Match -> 304 Not Modified
        let cached_req = test::TestRequest::get()
            .uri("/artifact/my-org/my-repo/v1.0.0/specs/openapi.json")
            .insert_header((header::IF_NONE_MATCH, returned_etag.as_str()))
            .to_request();
        let cached_resp = test::call_service(&app, cached_req).await;
        assert_eq!(cached_resp.status(), StatusCode::NOT_MODIFIED);

        // 6. Re-request with non-matching If-None-Match -> 200 OK
        let mismatch_req = test::TestRequest::get()
            .uri("/artifact/my-org/my-repo/v1.0.0/specs/openapi.json")
            .insert_header((header::IF_NONE_MATCH, r#""different-etag""#))
            .to_request();
        let mismatch_resp = test::call_service(&app, mismatch_req).await;
        assert_eq!(mismatch_resp.status(), StatusCode::OK);

        // 7. Re-request with non-UTF8 If-None-Match -> 200 OK
        let bad_inm_req = test::TestRequest::get()
            .uri("/artifact/my-org/my-repo/v1.0.0/specs/openapi.json")
            .insert_header((header::IF_NONE_MATCH, b"\xff\xff".as_slice()))
            .to_request();
        let bad_inm_resp = test::call_service(&app, bad_inm_req).await;
        assert_eq!(bad_inm_resp.status(), StatusCode::OK);

        // 8. Delete the artifact via DELETE /artifact/my-org/my-repo/v1.0.0/specs/openapi.json
        let delete_req = test::TestRequest::delete()
            .uri("/artifact/my-org/my-repo/v1.0.0/specs/openapi.json")
            .insert_header((header::AUTHORIZATION, "Bearer secret"))
            .to_request();
        let delete_resp = test::call_service(&app, delete_req).await;
        assert_eq!(delete_resp.status(), StatusCode::NO_CONTENT);

        // 9. Verify subsequent GET returns 404 Not Found
        let after_delete_req = test::TestRequest::get()
            .uri("/artifact/my-org/my-repo/v1.0.0/specs/openapi.json")
            .to_request();
        let after_delete_resp = test::call_service(&app, after_delete_req).await;
        assert_eq!(after_delete_resp.status(), StatusCode::NOT_FOUND);

        Ok(())
    }

    /// Verifies authentication, empty payload rejection, and path traversal rejection across endpoints.
    #[actix_web::test]
    async fn test_authentication_and_validation_rejections() -> Result<(), Box<dyn Error>> {
        let tmp_dir = TempDir::new()?;
        let store = LocalDiskStore::new(tmp_dir.path().to_path_buf());
        let state = web::Data::new(AppState::new(store, ApiKey::try_from("secret")?));

        let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, state))).await;

        // Missing Authorization header on upload_artifact
        let no_auth = test::TestRequest::post()
            .uri("/upload/my-org/my-repo/v1")
            .set_payload("data")
            .to_request();
        assert_eq!(
            test::call_service(&app, no_auth).await.status(),
            StatusCode::UNAUTHORIZED
        );

        // Wrong Bearer token on upload_named_artifact
        let wrong_auth = test::TestRequest::post()
            .uri("/upload/my-org/my-repo/v1/schema.json")
            .insert_header((header::AUTHORIZATION, "Bearer wrong"))
            .set_payload("data")
            .to_request();
        assert_eq!(
            test::call_service(&app, wrong_auth).await.status(),
            StatusCode::UNAUTHORIZED
        );

        // Invalid UTF-8 Authorization header on delete_artifact
        let bad_utf8_auth = test::TestRequest::delete()
            .uri("/artifact/my-org/my-repo/v1/schema.json")
            .insert_header((header::AUTHORIZATION, b"\xff\xff".as_slice()))
            .to_request();
        assert_eq!(
            test::call_service(&app, bad_utf8_auth).await.status(),
            StatusCode::UNAUTHORIZED
        );

        // Empty payload on upload_artifact -> 400 Bad Request
        let empty_default = test::TestRequest::post()
            .uri("/upload/my-org/my-repo/v1")
            .insert_header((header::AUTHORIZATION, "Bearer secret"))
            .to_request();
        assert_eq!(
            test::call_service(&app, empty_default).await.status(),
            StatusCode::BAD_REQUEST
        );

        // Empty payload on upload_named_artifact -> 400 Bad Request
        let empty_named = test::TestRequest::post()
            .uri("/upload/my-org/my-repo/v1/schema.json")
            .insert_header((header::AUTHORIZATION, "Bearer secret"))
            .to_request();
        assert_eq!(
            test::call_service(&app, empty_named).await.status(),
            StatusCode::BAD_REQUEST
        );

        // Invalid characters / path traversal in URL parameters -> 400 or 404
        let bad_org = test::TestRequest::post()
            .uri("/upload/bad$org/my-repo/v1")
            .insert_header((header::AUTHORIZATION, "Bearer secret"))
            .set_payload("data")
            .to_request();
        assert_eq!(
            test::call_service(&app, bad_org).await.status(),
            StatusCode::NOT_FOUND
        );

        Ok(())
    }

    /// Verifies deterministic `500 Internal Server Error` responses when the [`ArtifactStore`] returns I/O errors.
    #[actix_web::test]
    async fn test_deterministic_storage_io_errors() -> Result<(), Box<dyn Error>> {
        let state = web::Data::new(AppState::new(FailingStore, ApiKey::try_from("secret")?));
        let app = test::init_service(App::new().configure(|cfg| configure_app(cfg, state))).await;

        let put_default = test::TestRequest::post()
            .uri("/upload/my-org/my-repo/v1")
            .insert_header((header::AUTHORIZATION, "Bearer secret"))
            .set_payload("data")
            .to_request();
        assert_eq!(
            test::call_service(&app, put_default).await.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );

        let put_named = test::TestRequest::post()
            .uri("/upload/my-org/my-repo/v1/schema.json")
            .insert_header((header::AUTHORIZATION, "Bearer secret"))
            .set_payload("data")
            .to_request();
        assert_eq!(
            test::call_service(&app, put_named).await.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );

        let get_req = test::TestRequest::get()
            .uri("/artifact/my-org/my-repo/v1/schema.json")
            .to_request();
        assert_eq!(
            test::call_service(&app, get_req).await.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );

        let delete_req = test::TestRequest::delete()
            .uri("/artifact/my-org/my-repo/v1/schema.json")
            .insert_header((header::AUTHORIZATION, "Bearer secret"))
            .to_request();
        assert_eq!(
            test::call_service(&app, delete_req).await.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );

        Ok(())
    }
}
