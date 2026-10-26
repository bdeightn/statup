//! The key a request presents in `Authorization: Bearer`, the only proof the
//! API takes. Never in the address, which ends up in server and proxy logs.

use axum::extract::{FromRef, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use chrono::Utc;

use crate::db::DbPool;
use crate::error::AppError;
use crate::models::{ApiKeyHolder, Role, User};
use crate::repositories::UserRepository;
use crate::services::ApiKeyService;

/// A request made with a key in use. Refused with a 401 otherwise.
pub(super) struct ApiCaller {
    /// The key, which signs what a write leaves in the Journal and on the
    /// page.
    pub(super) key: ApiKeyHolder,
}

impl<S> FromRequestParts<S> for ApiCaller
where
    S: Send + Sync,
    DbPool: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let presented = bearer_key(&parts.headers).ok_or(AppError::Unauthorized)?;
        Self::presenting(&DbPool::from_ref(state), presented).await
    }
}

impl ApiCaller {
    /// The caller whose key this is, refused unless the key is in use. The
    /// reports read it from the body when the tool cannot set a header.
    pub(super) async fn presenting(pool: &DbPool, presented: &str) -> Result<Self, AppError> {
        let holder = ApiKeyService::holder_of(pool, presented)
            .await?
            .ok_or(AppError::Unauthorized)?;
        // The date on the keys page is a convenience: missing it costs the
        // request nothing.
        if let Err(e) = ApiKeyService::record_use(pool, &holder, Utc::now()).await {
            tracing::warn!(error = %e, "The use of an API key was not recorded");
        }
        Ok(Self { key: holder })
    }

    /// The administrator who made the key, with an editor's rights only: a
    /// key does what an editor may, never what only an administrator may,
    /// such as changing a closed event.
    pub(super) async fn editor(&self, pool: &DbPool) -> Result<User, AppError> {
        let mut owner = UserRepository::find_by_id(pool, self.key.created_by)
            .await?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("API key {} has no creator", self.key.id))
            })?;
        owner.role = Role::Publisher;
        Ok(owner)
    }
}

/// The key of an `Authorization: Bearer <key>` header. The scheme is read
/// without case, as HTTP schemes are.
pub(super) fn bearer_key(headers: &HeaderMap) -> Option<&str> {
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
