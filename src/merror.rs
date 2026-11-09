use std::{
    env::VarError,
    error::Error as StdError,
    fmt::{Display, Error as FmtError, Formatter, Result as FmtResult},
    io::Error as IoError,
    net::AddrParseError,
    num::TryFromIntError,
    string::FromUtf8Error,
};

use axum::{
    Json, http::StatusCode, http::header::ToStrError as HeaderToStrError, response::IntoResponse,
};
use serde_json::Error as SerdeJsonError;
use tokio::task::JoinError;

pub type MResult<T> = std::result::Result<T, MError>;

#[derive(Debug)]
pub enum MError {
    BadRequest(String),
    Conflict(String),
    NotFound(String),
    PermissionDenied(String),
    Unauthenticated(String),
    DeadlineExceeded(String),
    Internal(String),
}

impl MError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn permission_denied(msg: impl Into<String>) -> Self {
        Self::PermissionDenied(msg.into())
    }

    pub fn unauthenticated(msg: impl Into<String>) -> Self {
        Self::Unauthenticated(msg.into())
    }

    pub fn deadline_exceeded(msg: impl Into<String>) -> Self {
        Self::DeadlineExceeded(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

impl Display for MError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::BadRequest(msg) => write!(f, "Bad request: {msg}"),
            Self::Conflict(msg) => write!(f, "Conflict: {msg}"),
            Self::NotFound(msg) => write!(f, "Not found: {msg}"),
            Self::PermissionDenied(msg) => write!(f, "Permission denied: {msg}"),
            Self::Unauthenticated(msg) => write!(f, "Unauthenticated: {msg}"),
            Self::DeadlineExceeded(msg) => write!(f, "Deadline exceeded: {msg}"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl std::error::Error for MError {}

impl IntoResponse for MError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            Self::PermissionDenied(msg) => (StatusCode::FORBIDDEN, msg),
            Self::Unauthenticated(msg) => (StatusCode::UNAUTHORIZED, msg),
            Self::DeadlineExceeded(msg) => (StatusCode::GATEWAY_TIMEOUT, msg),
            Self::Internal(msg) => {
                tracing::error!("Internal error: {msg}");

                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        };

        // Most errors carry a plain-text message; some (per-plan token limits,
        // model_not_allowed) pass a JSON object so clients can read structured
        // fields (`code`, `remaining_tokens`). Return it as-is when parseable.
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&message)
            && value.is_object()
        {
            return (status, Json(value)).into_response();
        }

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

pub trait MResultExt<T> {
    /// Wraps the error in an [`MError::Internal`] with the given context message.
    ///
    /// # Errors
    ///
    /// Returns [`MError::Internal`] if `self` is `Err`.
    fn context(self, msg: &str) -> MResult<T>;

    /// Wraps the error in an [`MError::Internal`] with a lazily-computed context message.
    ///
    /// # Errors
    ///
    /// Returns [`MError::Internal`] if `self` is `Err`.
    fn with_context<C, F>(self, f: F) -> MResult<T>
    where
        F: FnOnce() -> C,
        C: Display;
}

impl<T, E: Display> MResultExt<T> for std::result::Result<T, E> {
    fn context(self, msg: &str) -> MResult<T> {
        self.map_err(|e| MError::internal(format!("{msg}: {e}")))
    }

    fn with_context<C, F>(self, f: F) -> MResult<T>
    where
        F: FnOnce() -> C,
        C: Display,
    {
        self.map_err(|e| MError::internal(format!("{}: {e}", f())))
    }
}

macro_rules! impl_from_for_le_error {
    ( $( $error_type:ty ),* ) => {
        $(
            impl From<$error_type> for MError {
                fn from(err: $error_type) -> Self {
                    MError::internal(err.to_string())
                }
            }
        )*
    };
}

impl_from_for_le_error!(
    &str,
    String,
    Box<dyn StdError>,
    FromUtf8Error,
    VarError,
    IoError,
    AddrParseError,
    JoinError,
    FmtError,
    TryFromIntError,
    HeaderToStrError,
    SerdeJsonError
);

pub fn bad_request(msg: impl Into<String>) -> MError {
    MError::bad_request(msg)
}

pub fn already_exists(msg: impl Into<String>) -> MError {
    MError::conflict(msg)
}

pub fn not_found(msg: impl Into<String>) -> MError {
    MError::not_found(msg)
}

pub fn permission_denied(msg: impl Into<String>) -> MError {
    MError::permission_denied(msg)
}

pub fn unauthenticated(msg: impl Into<String>) -> MError {
    MError::unauthenticated(msg)
}

pub fn deadline_exceeded(msg: impl Into<String>) -> MError {
    MError::deadline_exceeded(msg)
}

pub fn internal(msg: impl Into<String>) -> MError {
    MError::internal(msg)
}
