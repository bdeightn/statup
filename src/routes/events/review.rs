//! The review of a resolved incident, published, corrected and removed from
//! the incident's page.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::detail::{Composer, ReviewForm, detail_page};
use crate::error::AppError;
use crate::i18n::Locale;
use crate::middleware::{CsrfToken, HtmlForm, RequirePublisher};
use crate::routes::render;
use crate::services::EventService;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ReviewInput {
    #[serde(default)]
    body: String,
}

/// A refused review comes back in its form, open, with why.
pub async fn publish_review(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    csrf_token: CsrfToken,
    Locale(i18n): Locale,
    HtmlForm(input): HtmlForm<ReviewInput>,
) -> Result<Response, AppError> {
    match EventService::publish_review(&state.pool, id, &input.body, &user).await {
        Ok(()) => Ok(Redirect::to(&format!("/events/{id}?published=1")).into_response()),
        Err(AppError::Validation(key)) => {
            let review_form = ReviewForm {
                error: Some(i18n.t(&key).to_string()),
                body: input.body,
            };
            let page = detail_page(
                &state,
                id,
                Some(&user),
                csrf_token.0,
                i18n,
                Composer::default(),
                review_form,
            )
            .await?;
            render(&page)
        }
        Err(e) => Err(e),
    }
}

pub async fn remove_review(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    EventService::remove_review(&state.pool, id, user.role).await?;
    Ok(Redirect::to(&format!("/events/{id}")).into_response())
}
