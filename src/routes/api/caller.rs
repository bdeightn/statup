//! The key a request presents in `Authorization: Bearer`, the only proof the
//! API takes. Never in the address, which ends up in server and proxy logs.

use axum::extract::{FromRef, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use chrono::Utc;

use crate::db::DbPool;
use crate::error::AppError;
use crate::services::ApiKeyService;

/// A request made with a key in use. Refused with a 401 otherwise.
pub(super) struct ApiCaller;

impl<S> FromRequestParts<S> for ApiCaller
where
    S: Send + Sync,
    DbPool: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let presented = bearer_key(&parts.headers).ok_or(AppError::Unauthorized)?;
        let pool = DbPool::from_ref(state);
        let holder = ApiKeyService::holder_of(&pool, presented)
            .await?
            .ok_or(AppError::Unauthorized)?;
        // The date on the keys page is a convenience: missing it costs the
        // request nothing.
        if let Err(e) = ApiKeyService::record_use(&pool, &holder, Utc::now()).await {
            tracing::warn!(error = %e, "The use of an API key was not recorded");
        }
        Ok(Self)
    }
}

/// The key of an `Authorization: Bearer <key>` header. The scheme is read
/// without case, as HTTP schemes are.
fn bearer_key(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, key) = value.trim().split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| key.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn key_in(value: &str) -> Option<String> {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(value).unwrap());
        bearer_key(&headers).map(ToOwned::to_owned)
    }

    #[test]
    fn the_key_is_read_from_a_bearer_header_only() {
        assert_eq!(key_in("Bearer stp_abc").as_deref(), Some("stp_abc"));
        assert_eq!(key_in("bearer  stp_abc ").as_deref(), Some("stp_abc"));
        assert_eq!(key_in("BEARER stp_abc").as_deref(), Some("stp_abc"));
        assert_eq!(key_in("Basic c3RwX2FiYw=="), None);
        assert_eq!(key_in("stp_abc"), None);
        assert_eq!(bearer_key(&HeaderMap::new()), None);
    }
}
