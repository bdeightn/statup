//! Saved templates offered by the new event form.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use crate::error::AppError;
use crate::i18n::{I18n, Locale};
use crate::middleware::RequirePublisher;
use crate::models::{Category, JournalKind, Severity};
use crate::repositories::{EventTemplateRepository, NewJournalEntry};
use crate::routes::render;
use crate::services::EventTemplateService;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "events/template_suggestions.html")]
struct TemplateSuggestionsTemplate {
    templates: Vec<crate::models::EventTemplate>,
    i18n: I18n,
}

#[derive(Deserialize)]
pub struct TemplateSearchQuery {
    #[serde(default, alias = "title")]
    q: String,
}

pub async fn template_search(
    _publisher: RequirePublisher,
    Locale(i18n): Locale,
    State(state): State<AppState>,
    Query(params): Query<TemplateSearchQuery>,
) -> Result<Response, AppError> {
    let query = params.q.trim();
    let templates = if query.chars().count() >= 2 {
        EventTemplateRepository::search_by_title(&state.pool, query, 5).await?
    } else {
        Vec::new()
    };
    render(&TemplateSuggestionsTemplate { templates, i18n })
}

pub async fn template_detail(
    _publisher: RequirePublisher,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let template = EventTemplateRepository::find_by_id(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let body = serde_json::json!({
        "id": template.id,
        "title": template.title,
        "description": template.description,
        "kind": template.kind.as_str(),
        "severity": template.severity.map(Severity::as_str),
        "planned": template.planned,
        "category": template.category.map(Category::as_str),
    });
    Ok(axum::Json(body).into_response())
}

/// Removed from the template menu of a new event, which opens again.
pub async fn template_delete(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let template = EventTemplateRepository::find_by_id(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    EventTemplateService::delete(&state.pool, id).await?;
    NewJournalEntry::new(JournalKind::TemplateDeleted)
        .by(&user)
        .about(&template.title)
        .record(&state.pool)
        .await;
    Ok(Redirect::to("/events/new").into_response())
}
