//! Every refusal of the API as JSON, `{"error": {"code", "message"}}`.
//! Handlers answer with `AppError` as the pages do; this layer says it in
//! the body a script reads, along with the bare statuses that the layers
//! and extractors answer on their own. Messages are in English: they are
//! part of the API, not of the page.

use axum::Json;
use axum::extract::Request;
use axum::http::header::{RETRY_AFTER, WWW_AUTHENTICATE};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::error::error_key;
use crate::i18n::I18n;
use crate::middleware::rate_limit::RateLimited;

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: String,
    message: String,
}

pub(super) async fn json_errors(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    if !status.is_client_error() && !status.is_server_error() {
        return response;
    }
    let limited = response.extensions().get::<RateLimited>().copied();
    let key = error_key(&response);
    let code = match (limited, key) {
        (Some(_), _) => "rate_limited",
        (None, Some(key)) => code_of_key(key),
        (None, None) => code_of_status(status),
    };
    let error = ErrorDetail {
        code: code.to_string(),
        message: message_of(code, key),
    };
    let mut answer = (status, Json(ErrorBody { error })).into_response();
    let headers = answer.headers_mut();
    if let Some(limited) = limited {
        headers.insert(RETRY_AFTER, HeaderValue::from(limited.retry_after_secs));
    }
    if status == StatusCode::UNAUTHORIZED {
        headers.insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    }
    answer
}

/// The code of an error named by its translation key: what follows the
/// namespace, `error.not_found` gives `not_found`.
fn code_of_key(key: &str) -> &str {
    key.split_once('.').map_or(key, |(_, code)| code)
}

/// The code of a status answered without an `AppError`: by the body limit,
/// the timeout, the router, or an extractor that could not read the request.
fn code_of_status(status: StatusCode) -> &'static str {
    match status {
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::METHOD_NOT_ALLOWED => "method_not_allowed",
        StatusCode::REQUEST_TIMEOUT => "timeout",
        StatusCode::PAYLOAD_TOO_LARGE => "payload_too_large",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "unsupported_media_type",
        status if status.is_server_error() => "internal",
        _ => "invalid_data",
    }
}

/// What the API says for the errors any route can meet; any other code is
/// a refusal named by its key, said in its English text.
fn message_of(code: &str, key: Option<&str>) -> String {
    let message = match code {
        "unauthorized" => "Send a valid API key in the Authorization header: Bearer <key>.",
        "not_found" => "Nothing exists at this address.",
        "method_not_allowed" => "This address does not take this method.",
        "invalid_data" => "The request could not be read.",
        "unsupported_media_type" => "Send the body as JSON, with Content-Type: application/json.",
        "payload_too_large" => "The request body is larger than 64 KB.",
        "timeout" => "The server took too long to answer.",
        "rate_limited" => {
            "Too many requests from this address. Retry after the delay in Retry-After."
        }
        "internal" => "An internal error occurred.",
        _ => return key.map_or(code, |key| I18n::new("en").t(key)).to_string(),
    };
    message.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_is_the_key_without_its_namespace() {
        assert_eq!(code_of_key("error.not_found"), "not_found");
        assert_eq!(
            code_of_key("validation.api_key_name_taken"),
            "api_key_name_taken"
        );
        assert_eq!(code_of_key("plain"), "plain");
    }

    #[test]
    fn a_refusal_says_its_english_text() {
        assert_eq!(
            message_of(
                "api_key_name_required",
                Some("validation.api_key_name_required")
            ),
            I18n::new("en").t("validation.api_key_name_required")
        );
        assert_ne!(
            message_of("unauthorized", Some("error.unauthorized")),
            I18n::new("en").t("error.unauthorized"),
            "the API does not ask a script to sign in"
        );
    }

    #[test]
    fn a_bare_status_has_a_code() {
        assert_eq!(
            code_of_status(StatusCode::PAYLOAD_TOO_LARGE),
            "payload_too_large"
        );
        assert_eq!(
            code_of_status(StatusCode::UNSUPPORTED_MEDIA_TYPE),
            "unsupported_media_type"
        );
        assert_eq!(
            code_of_status(StatusCode::UNPROCESSABLE_ENTITY),
            "invalid_data"
        );
        assert_eq!(code_of_status(StatusCode::BAD_GATEWAY), "internal");
    }
}
