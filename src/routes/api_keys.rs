//! The API keys, in the settings: a key made with a name and shown once,
//! the keys in use, a key revoked.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{Redirect, Response};
use chrono::Utc;
use serde::Deserialize;

use super::{Frame, render};
use crate::clock;
use crate::error::AppError;
use crate::i18n::{I18n, Locale};
use crate::middleware::headers::no_store;
use crate::middleware::{CsrfToken, HtmlForm, RequireAdmin};
use crate::models::{ApiKey, JournalKind, User};
use crate::repositories::{ApiKeyRepository, NewJournalEntry};
use crate::services::{ApiKeyService, NewApiKey};
use crate::state::AppState;

#[derive(Template)]
#[template(path = "admin/api_keys.html")]
struct ApiKeysTemplate {
    frame: Frame,
    rows: Vec<KeyRow>,
    page: Page,
    i18n: I18n,
}

impl ApiKeysTemplate {
    /// The form opens again to show why a name was refused.
    fn add_open(&self) -> bool {
        self.page.error.is_some()
    }
}

/// A key in use as the list shows it.
struct KeyRow {
    id: i64,
    name: String,
    prefix: String,
    /// "Made by Karl on 26 Oct".
    made: String,
    creator_disabled: bool,
    last_used: Option<String>,
}

impl KeyRow {
    fn of(key: ApiKey, i18n: &I18n) -> Self {
        let date = i18n.format_date_short(&clock::local_date(&key.created_at));
        Self {
            id: key.id,
            made: i18n.tf(
                "api_keys.created_by",
                &[("name", &key.creator_name), ("date", &date)],
            ),
            creator_disabled: !key.creator_active,
            last_used: key.last_used_at.map(|at| i18n.format_datetime(&at)),
            name: key.name,
            prefix: key.prefix,
        }
    }
}

/// What the page says above the keys.
#[derive(Default)]
struct Page {
    /// A key just made, or given a new secret, shown this once.
    created: Option<NewApiKey>,
    regenerated: bool,
    revoked: bool,
    /// A refused name: why, and the name to show again in the open form.
    error: Option<String>,
    name: String,
}

/// `?revoked` only says that a key was revoked, never which: the notice
/// takes nothing from the address. Any value is read as text, so an old
/// link still opens the page.
#[derive(Deserialize)]
pub struct ListQuery {
    revoked: Option<String>,
}

#[derive(Deserialize)]
pub struct NewKeyInput {
    #[serde(default)]
    name: String,
}

pub async fn list(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
    Query(query): Query<ListQuery>,
) -> Result<Response, AppError> {
    let page = Page {
        revoked: query.revoked.is_some(),
        ..Page::default()
    };
    render_page(&state, &admin, csrf_token, i18n, page).await
}

/// The key is shown in the answer to the form and kept nowhere but as a
/// hash. The page then takes the list's address, so a reload does not send
/// the form again.
pub async fn create(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
    HtmlForm(input): HtmlForm<NewKeyInput>,
) -> Result<Response, AppError> {
    match ApiKeyService::create(&state.pool, &input.name, &admin).await {
        Ok(made) => {
            NewJournalEntry::new(JournalKind::ApiKeyCreated)
                .by(&admin)
                .about(&made.name)
                .record(&state.pool)
                .await;
            let page = Page {
                created: Some(made),
                ..Page::default()
            };
            Ok(no_store(
                render_page(&state, &admin, csrf_token, i18n, page).await?,
            ))
        }
        Err(AppError::Validation(key)) => {
            let page = Page {
                error: Some(i18n.t(&key).to_string()),
                name: input.name,
                ..Page::default()
            };
            render_page(&state, &admin, csrf_token, i18n, page).await
        }
        Err(e) => Err(e),
    }
}

/// A new secret for a key, shown once like a new key; the old one stops
/// working at once.
pub async fn regenerate(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let made = ApiKeyService::regenerate(&state.pool, id).await?;
    NewJournalEntry::new(JournalKind::ApiKeyRegenerated)
        .by(&admin)
        .about(&made.name)
        .record(&state.pool)
        .await;
    let page = Page {
        created: Some(made),
        regenerated: true,
        ..Page::default()
    };
    Ok(no_store(
        render_page(&state, &admin, csrf_token, i18n, page).await?,
    ))
}

pub async fn revoke(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Redirect, AppError> {
    let name = ApiKeyRepository::revoke(&state.pool, id, Utc::now())
        .await?
        .ok_or(AppError::NotFound)?;
    NewJournalEntry::new(JournalKind::ApiKeyRevoked)
        .by(&admin)
        .about(name)
        .record(&state.pool)
        .await;
    Ok(Redirect::to("/admin/api-keys?revoked=1"))
}

async fn render_page(
    state: &AppState,
    admin: &User,
    csrf_token: String,
    i18n: I18n,
    page: Page,
) -> Result<Response, AppError> {
    let rows = ApiKeyRepository::list_in_use(&state.pool)
        .await?
        .into_iter()
        .map(|key| KeyRow::of(key, &i18n))
        .collect();
    let frame = Frame::load(&state.pool, Some(admin), csrf_token, &i18n).await?;
    render(&ApiKeysTemplate {
        frame,
        rows,
        page,
        i18n,
    })
}
