use axum::http::StatusCode;
use orchy_core::error::Error;

pub fn error_to_code_and_message(e: &Error) -> (StatusCode, &'static str, String) {
    use orchy_core::error::DomainError;
    let (status, code) = match e {
        Error::NotFound { .. } => (StatusCode::NOT_FOUND, "NOT_FOUND"),
        Error::Domain(
            DomainError::Validation(_)
            | DomainError::InvalidTransition { .. }
            | DomainError::DependencyNotMet(_),
        ) => (StatusCode::UNPROCESSABLE_ENTITY, "INVALID_INPUT"),
        Error::Conflict(_) | Error::VersionMismatch { .. } => (StatusCode::CONFLICT, "CONFLICT"),
        Error::Store(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        Error::Domain(DomainError::PasswordMismatch | DomainError::Deactivated) => {
            (StatusCode::UNAUTHORIZED, "UNAUTHORIZED")
        }
        Error::Domain(_) => (StatusCode::UNPROCESSABLE_ENTITY, "INVALID_INPUT"),
    };
    (status, code, e.to_string())
}

pub fn error_to_mcp_json(e: &Error) -> String {
    let (_, code, message) = error_to_code_and_message(e);
    serde_json::json!({ "error": { "code": code, "message": message } }).to_string()
}
