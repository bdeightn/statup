//! The Journal page of the administrators: what was kept, its search, and
//! what the journal captures and for how long.

use std::fmt::Write as _;

use askama::Template;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::events::deserialize_blank_as_none;
use super::{Frame, render};
use crate::clock;
use crate::error::AppError;
use crate::i18n::{I18n, Locale};
use crate::journal;
use crate::middleware::headers::is_htmx;
use crate::middleware::{CsrfToken, HtmlForm, RequireAdmin};
use crate::models::{
    JournalCategory, JournalDay, JournalFilters, JournalKind, User, group_journal_by_day,
};
use crate::repositories::{JournalRepository, NewJournalEntry, UserRepository};
use crate::state::AppState;

const PAGE_SIZE: i64 = 50;
const MAX_PAGE: i64 = 10_000;

#[derive(Deserialize, Default)]
pub struct JournalQuery {
    page: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_blank_as_none")]
    category: Option<String>,
    #[serde(default, deserialize_with = "deserialize_blank_as_none")]
    actor_id: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_blank_as_none")]
    from: Option<String>,
    #[serde(default, deserialize_with = "deserialize_blank_as_none")]
    to: Option<String>,
    #[serde(default, deserialize_with = "deserialize_blank_as_none")]
    q: Option<String>,
    saved: Option<String>,
}

impl JournalQuery {
    fn page(&self) -> i64 {
        self.page.unwrap_or(1).clamp(1, MAX_PAGE)
    }

    fn category(&self) -> Option<JournalCategory> {
        self.category.as_deref().and_then(JournalCategory::parse)
    }

    fn query(&self) -> Option<String> {
        self.q
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(String::from)
    }

    fn filters(&self) -> JournalFilters {
        JournalFilters {
            category: self.category(),
            actor_id: self.actor_id,
            from: self
                .from
                .as_deref()
                .and_then(|d| clock::parse_day_bound(d, false)),
            to: self
                .to
                .as_deref()
                .and_then(|d| clock::parse_day_bound(d, true)),
            q: self.query(),
            limit: PAGE_SIZE + 1,
            offset: (self.page() - 1) * PAGE_SIZE,
        }
    }

    fn has_filters(&self) -> bool {
        self.query().is_some() || self.refinements() > 0
    }

    /// The filters set besides the search: what a phone folds away.
    fn refinements(&self) -> usize {
        [
            self.category().is_some(),
            self.actor_id.is_some(),
            self.from.is_some(),
            self.to.is_some(),
        ]
        .into_iter()
        .filter(|set| *set)
        .count()
    }

    /// Query string of the current filters, ready for a `page=` parameter.
    fn page_link_base(&self) -> String {
        let mut url = String::from("/admin/journal?");
        let pairs = [
            ("category", self.category().map(|c| c.as_str().to_string())),
            ("actor_id", self.actor_id.map(|id| id.to_string())),
            ("q", self.query()),
            ("from", self.from.clone()),
            ("to", self.to.clone()),
        ];
        for (name, value) in pairs {
            if let Some(value) = value {
                let encoded = percent_encoding::utf8_percent_encode(
                    &value,
                    percent_encoding::NON_ALPHANUMERIC,
                );
                let _ = write!(url, "{name}={encoded}&");
            }
        }
        url
    }
}

/// The part of the page that the filters refresh.
pub struct JournalResults {
    pub days: Vec<JournalDay>,
    pub total: i64,
    pub count_label: String,
    pub page: i64,
    pub page_label: String,
    pub has_next: bool,
    pub page_link_base: String,
    pub has_filters: bool,
}

/// What the journal keeps: said in one line beside the title, and in the
/// fields under it once opened, with a refused choice as it was typed.
struct Capture {
    /// "Essential capture".
    summary: String,
    /// "90 days".
    kept: String,
    detailed: bool,
    retention_days: String,
    open: bool,
    saved: bool,
    error: Option<String>,
}

impl Capture {
    async fn saved(state: &AppState, i18n: &I18n, query: &JournalQuery) -> Self {
        let detailed = journal::captures_details(&state.pool).await;
        let days = journal::retention_days(&state.pool).await;
        Self {
            summary: summary(i18n, detailed),
            kept: i18n.plural("journal.days", usize::try_from(days).unwrap_or(0)),
            detailed,
            retention_days: days.to_string(),
            open: false,
            saved: query.saved.is_some(),
            error: None,
        }
    }
}

fn summary(i18n: &I18n, detailed: bool) -> String {
    let key = if detailed {
        "journal.summary_detailed"
    } else {
        "journal.summary_essential"
    };
    i18n.t(key).to_string()
}

#[derive(Template)]
#[template(path = "admin/journal.html")]
struct JournalPageTemplate {
    frame: Frame,
    results: JournalResults,
    query: JournalQuery,
    members: Vec<User>,
    capture: Capture,
    i18n: I18n,
}

impl JournalPageTemplate {
    fn category_is(&self, category: &str) -> bool {
        self.query
            .category()
            .is_some_and(|c| c.as_str() == category)
    }

    fn categories(&self) -> Vec<(&'static str, &str)> {
        JournalCategory::ALL
            .into_iter()
            .map(|c| (c.as_str(), self.i18n.t(c.label_key())))
            .collect()
    }

    // Askama hands a field over by reference.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn member_is(&self, id: &i64) -> bool {
        self.query.actor_id == Some(*id)
    }

    fn refinements(&self) -> usize {
        self.query.refinements()
    }
}

#[derive(Template)]
#[template(path = "admin/_journal_results.html")]
struct JournalResultsTemplate {
    results: JournalResults,
    i18n: I18n,
}

async fn load_results(
    state: &AppState,
    query: &JournalQuery,
    i18n: &I18n,
) -> Result<JournalResults, AppError> {
    let filters = query.filters();
    let mut entries = JournalRepository::list_page(&state.pool, &filters).await?;
    let total = JournalRepository::count_filtered(&state.pool, &filters).await?;
    let page_size = usize::try_from(PAGE_SIZE).unwrap_or(usize::MAX);
    let has_next = entries.len() > page_size;
    entries.truncate(page_size);
    Ok(JournalResults {
        days: group_journal_by_day(entries, i18n),
        total,
        count_label: i18n.plural("journal.count", usize::try_from(total).unwrap_or(0)),
        page: query.page(),
        page_label: i18n.tf("common.page_n", &[("n", &query.page().to_string())]),
        has_next,
        page_link_base: query.page_link_base(),
        has_filters: query.has_filters(),
    })
}

pub async fn page(
    RequireAdmin(user): RequireAdmin,
    State(state): State<AppState>,
    Query(query): Query<JournalQuery>,
    headers: HeaderMap,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
) -> Result<Response, AppError> {
    let results = load_results(&state, &query, &i18n).await?;
    if is_htmx(&headers) {
        return render(&JournalResultsTemplate { results, i18n });
    }
    let capture = Capture::saved(&state, &i18n, &query).await;
    render_page(&state, &user, csrf_token, i18n, query, results, capture).await
}

async fn render_page(
    state: &AppState,
    user: &User,
    csrf_token: String,
    i18n: I18n,
    query: JournalQuery,
    results: JournalResults,
    capture: Capture,
) -> Result<Response, AppError> {
    let frame = Frame::load(&state.pool, Some(user), csrf_token, &i18n).await?;
    render(&JournalPageTemplate {
        frame,
        results,
        query,
        members: UserRepository::list_all(&state.pool).await?,
        capture,
        i18n,
    })
}

#[derive(Deserialize)]
pub struct CaptureForm {
    capture: String,
    retention_days: String,
}

/// Saves what the journal captures and how long it keeps it. A refused
/// retention shows the page again with what was typed.
pub async fn save_settings(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
    HtmlForm(form): HtmlForm<CaptureForm>,
) -> Result<Response, AppError> {
    let detailed = form.capture == "detailed";
    let days = form.retention_days.trim().parse().unwrap_or(0);
    match journal::save_settings(&state.pool, detailed, days).await {
        Ok(()) => {
            let capture = if detailed { "detailed" } else { "essential" };
            let entry = NewJournalEntry::new(JournalKind::JournalSettingsSaved)
                .by(&admin)
                .with_detail(format!("capture={capture}, retention={days} days"));
            journal::record(&state.pool, entry).await;
            Ok(Redirect::to("/admin/journal?saved=1").into_response())
        }
        Err(AppError::Validation(key)) => {
            let query = JournalQuery::default();
            let results = load_results(&state, &query, &i18n).await?;
            let capture = Capture {
                detailed,
                retention_days: form.retention_days.trim().to_string(),
                open: true,
                error: Some(i18n.t(&key).to_string()),
                ..Capture::saved(&state, &i18n, &query).await
            };
            render_page(&state, &admin, csrf_token, i18n, query, results, capture).await
        }
        Err(e) => Err(e),
    }
}

/// What the Journal tile of the settings says: what is captured, and for
/// how long.
pub async fn tile_label(state: &AppState, i18n: &I18n) -> String {
    let capture = summary(i18n, journal::captures_details(&state.pool).await);
    let days = usize::try_from(journal::retention_days(&state.pool).await).unwrap_or(0);
    format!("{capture}, {}", i18n.plural("journal.days", days))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(pairs: &[(&str, &str)]) -> JournalQuery {
        let raw = pairs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        serde_html_form::from_str(&raw).unwrap()
    }

    #[test]
    fn blank_filters_are_left_out() {
        let q = query(&[("category", ""), ("actor_id", ""), ("q", "  ")]);
        assert!(!q.has_filters());
        assert_eq!(q.page_link_base(), "/admin/journal?");
    }

    #[test]
    fn an_unknown_family_is_ignored_and_the_others_are_kept() {
        let q = query(&[("category", "nope"), ("actor_id", "3"), ("q", "disk full")]);
        assert_eq!(q.category(), None);
        assert_eq!(q.refinements(), 1);
        assert_eq!(
            q.page_link_base(),
            "/admin/journal?actor_id=3&q=disk%20full&"
        );
    }
}
