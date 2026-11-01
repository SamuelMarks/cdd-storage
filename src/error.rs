//! Application error types.
//!
//! This module defines the unified [`AppError`] enumeration used across the
//! entire `cdd-storage` crate for domain validation, configuration parsing,
//! storage backend I/O, and HTTP response mapping.

use actix_web::{HttpResponse, ResponseError, http::StatusCode};

/// Centralized error type for the `cdd-storage` application.
#[derive(Debug, derive_more::Display, derive_more::Error, derive_more::From)]
pub enum AppError {
    /// An invalid domain identifier or parameter was provided.
    #[display("Invalid {field}: {reason}")]
    InvalidIdentifier {
        /// The name of the invalid field.
        field: &'static str,
        /// The human-readable reason why validation failed.
        reason: &'static str,
    },

    /// A path traversal or unsafe path component was detected.
    #[display("Path traversal attempt or unsafe path rejected")]
    PathTraversalAttempt,

    /// An upload payload was empty when non-empty data was required.
    #[display("Payload must not be empty")]
    EmptyPayload,

    /// Application configuration was invalid.
    #[display("Configuration error: {_0}")]
    #[from(ignore)]
    ConfigurationError(#[error(not(source))] String),

    /// A storage input/output error occurred.
    #[display("Storage IO Error: {_0}")]
    IoError(#[error(source)] std::io::Error),

    /// The requested artifact resource was not found.
    #[display("Not Found")]
    NotFound,

    /// Authentication or authorization failed.
    #[display("Unauthorized")]
    Unauthorized,
}

/// Type alias for [`AppError`] representing storage and HTTP errors across `cdd-storage`.
pub type StorageError = AppError;

impl ResponseError for AppError {
    /// Returns the appropriate HTTP [`StatusCode`] for this error.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`AppError`] instance being converted to a status code.
    ///
    /// # Returns
    ///
    /// Returns the corresponding [`StatusCode`]:
    /// - `400 Bad Request` for validation, path traversal, or empty payload errors.
    /// - `401 Unauthorized` for authentication failures.
    /// - `404 Not Found` for missing artifacts.
    /// - `500 Internal Server Error` for configuration or I/O errors.
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidIdentifier { .. } | Self::PathTraversalAttempt | Self::EmptyPayload => {
                StatusCode::BAD_REQUEST
            }
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::ConfigurationError(_) | Self::IoError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Creates an [`HttpResponse`] representing this error.
    ///
    /// # Arguments
    ///
    /// * `&self` - The [`AppError`] instance being rendered into an HTTP response.
    ///
    /// # Returns
    ///
    /// Returns an [`HttpResponse`] with the status code from [`Self::status_code`]
    /// and a UTF-8 plain-text body matching [`std::fmt::Display`].
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).body(self.to_string())
    }
}

#[cfg(test)]
/// Unit tests for [`AppError`].
mod tests {
    use super::*;
    use actix_web::body::to_bytes;
    use std::error::Error as StdError;
    use std::io;

    /// Verifies the `Display` formatting and `Error::source` behavior for all [`AppError`] variants.
    #[test]
    fn test_app_error_display_and_source() {
        let invalid_id = AppError::InvalidIdentifier {
            field: "org_id",
            reason: "cannot be empty",
        };
        assert_eq!(invalid_id.to_string(), "Invalid org_id: cannot be empty");
        assert!(invalid_id.source().is_none());

        let traversal = AppError::PathTraversalAttempt;
        assert_eq!(
            traversal.to_string(),
            "Path traversal attempt or unsafe path rejected"
        );
        assert!(traversal.source().is_none());

        let empty = AppError::EmptyPayload;
        assert_eq!(empty.to_string(), "Payload must not be empty");
        assert!(empty.source().is_none());

        let cfg_err = AppError::ConfigurationError(String::from("invalid PORT"));
        assert_eq!(cfg_err.to_string(), "Configuration error: invalid PORT");
        assert!(cfg_err.source().is_none());

        let not_found = AppError::NotFound;
        assert_eq!(not_found.to_string(), "Not Found");
        assert!(not_found.source().is_none());

        let unauth = AppError::Unauthorized;
        assert_eq!(unauth.to_string(), "Unauthorized");
        assert!(unauth.source().is_none());

        let io_err = io::Error::other("disk failure");
        let app_io_err = AppError::from(io_err);
        assert_eq!(app_io_err.to_string(), "Storage IO Error: disk failure");
        assert!(app_io_err.source().is_some());
    }

    /// Verifies that every [`AppError`] variant maps to the expected HTTP [`StatusCode`] and body bytes.
    #[actix_web::test]
    async fn test_app_error_response_status_and_body() -> Result<(), Box<dyn StdError>> {
        let cases: Vec<(AppError, StatusCode, &str)> = vec![
            (
                AppError::InvalidIdentifier {
                    field: "repo_id",
                    reason: "invalid character",
                },
                StatusCode::BAD_REQUEST,
                "Invalid repo_id: invalid character",
            ),
            (
                AppError::PathTraversalAttempt,
                StatusCode::BAD_REQUEST,
                "Path traversal attempt or unsafe path rejected",
            ),
            (
                AppError::EmptyPayload,
                StatusCode::BAD_REQUEST,
                "Payload must not be empty",
            ),
            (
                AppError::ConfigurationError(String::from("missing API_KEY")),
                StatusCode::INTERNAL_SERVER_ERROR,
                "Configuration error: missing API_KEY",
            ),
            (
                AppError::from(io::Error::other("read failed")),
                StatusCode::INTERNAL_SERVER_ERROR,
                "Storage IO Error: read failed",
            ),
            (AppError::NotFound, StatusCode::NOT_FOUND, "Not Found"),
            (
                AppError::Unauthorized,
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
            ),
        ];

        for (err, expected_status, expected_body) in cases {
            assert_eq!(err.status_code(), expected_status);
            let resp = err.error_response();
            assert_eq!(resp.status(), expected_status);
            let body_bytes = to_bytes(resp.into_body()).await?;
            assert_eq!(body_bytes.as_ref(), expected_body.as_bytes());
        }

        Ok(())
    }
}
