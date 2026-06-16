//! Application error types.

use actix_web::{HttpResponse, ResponseError};

/// Centralized error type for the application.
#[derive(Debug, derive_more::Display, derive_more::Error, derive_more::From)]
pub enum AppError {
    /// A generic internal error.
    #[display("Internal Server Error")]
    InternalError,

    /// A storage input/output error.
    #[display("Storage IO Error: {_0}")]
    IoError(#[error(source)] std::io::Error),

    /// The requested resource was not found.
    #[display("Not Found")]
    NotFound,

    /// Unauthorized access.
    #[display("Unauthorized")]
    Unauthorized,
}

impl ResponseError for AppError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            Self::InternalError | Self::IoError(_) => {
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::NotFound => actix_web::http::StatusCode::NOT_FOUND,
            Self::Unauthorized => actix_web::http::StatusCode::UNAUTHORIZED,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).body(self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;
    use std::io;

    #[test]
    fn test_app_error_display() {
        assert_eq!(AppError::InternalError.to_string(), "Internal Server Error");
        assert_eq!(AppError::NotFound.to_string(), "Not Found");
        assert_eq!(AppError::Unauthorized.to_string(), "Unauthorized");

        let io_err = io::Error::other("test error");
        let app_err = AppError::from(io_err);
        assert_eq!(app_err.to_string(), "Storage IO Error: test error");
    }

    #[test]
    fn test_app_error_response_status() {
        assert_eq!(
            AppError::InternalError.status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(AppError::NotFound.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(
            AppError::Unauthorized.status_code(),
            StatusCode::UNAUTHORIZED
        );

        let io_err = io::Error::other("test error");
        let app_err = AppError::from(io_err);
        assert_eq!(app_err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_app_error_response_body() {
        let resp = AppError::InternalError.error_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let resp_not_found = AppError::NotFound.error_response();
        assert_eq!(resp_not_found.status(), StatusCode::NOT_FOUND);

        let resp_unauth = AppError::Unauthorized.error_response();
        assert_eq!(resp_unauth.status(), StatusCode::UNAUTHORIZED);
    }
}
