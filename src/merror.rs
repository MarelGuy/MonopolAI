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
use sqlx::Error as SqlxError;
use tokio::task::JoinError;

pub type MResult<T> = std::result::Result<T, MError>;

#[derive(Debug)]
pub enum MError {
    BadRequest(String),
    Conflict(String),
    NotFound(String),
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
            Self::Internal(msg) => {
                tracing::error!("Internal error: {msg}");

                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        };

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
}

impl<T, E: Display> MResultExt<T> for std::result::Result<T, E> {
    fn context(self, msg: &str) -> MResult<T> {
        self.map_err(|e| MError::internal(format!("{msg}: {e}")))
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
    SerdeJsonError,
    SqlxError
);

pub fn bad_request(msg: impl Into<String>) -> MError {
    MError::bad_request(msg)
}

// pub fn already_exists(msg: impl Into<String>) -> MError {
//     MError::conflict(msg)
// }

pub fn not_found(msg: impl Into<String>) -> MError {
    MError::not_found(msg)
}

// pub fn internal(msg: impl Into<String>) -> MError {
//     MError::internal(msg)
// }

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};
    use serde_json::{Value, json};

    use super::{MError, MResult, MResultExt};

    async fn response_parts(error: MError) -> (StatusCode, Value) {
        let response = error.into_response();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn maps_client_errors_to_statuses_and_json_messages() {
        let cases = [
            (
                MError::bad_request("bad input"),
                StatusCode::BAD_REQUEST,
                "bad input",
            ),
            (
                MError::conflict("already exists"),
                StatusCode::CONFLICT,
                "already exists",
            ),
            (
                MError::not_found("missing"),
                StatusCode::NOT_FOUND,
                "missing",
            ),
        ];

        for (error, expected_status, expected_message) in cases {
            let (status, body) = response_parts(error).await;
            assert_eq!(status, expected_status);
            assert_eq!(body, json!({ "error": expected_message }));
        }
    }

    #[tokio::test]
    async fn hides_internal_error_details() {
        let (status, body) = response_parts(MError::internal("secret database details")).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, json!({ "error": "Internal server error" }));
        assert!(!body.to_string().contains("secret database details"));
    }

    #[test]
    fn display_includes_the_error_kind() {
        assert_eq!(MError::bad_request("bad").to_string(), "Bad request: bad");
        assert_eq!(
            MError::conflict("duplicate").to_string(),
            "Conflict: duplicate"
        );
        assert_eq!(
            MError::not_found("missing").to_string(),
            "Not found: missing"
        );
        assert_eq!(
            MError::internal("broken").to_string(),
            "Internal error: broken"
        );
    }

    #[test]
    fn context_wraps_errors_and_passes_through_successes() {
        let error: MResult<()> = Err::<(), _>("boom").context("while loading");
        match error {
            Err(MError::Internal(message)) => assert_eq!(message, "while loading: boom"),
            _ => panic!("expected an internal error"),
        }

        let success: MResult<u8> = Ok::<_, &str>(1).context("while loading");
        assert_eq!(success.unwrap(), 1);
    }
}
