//! The email subscribers, for administrators: how many there are on the
//! Alerts page, their list, who may subscribe, and removing one.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::notifications::{LastSend, outcome_line};
use super::subscriptions::{address_of, choice_words};
use super::{Frame, render};
use crate::clock;
use crate::error::AppError;
use crate::i18n::{I18n, Locale};
use crate::middleware::{CsrfToken, HtmlForm, RequireAdmin};
use crate::models::{JournalKind, SEPARATOR, SubscriberRow};
use crate::repositories::{NewJournalEntry, SubscriberRepository};
use crate::services::SubscriptionService;
use crate::state::AppState;

/// What the Alerts page says of the subscribers.
pub(super) struct SubscribersCard {
    pub(super) has_any: bool,
    /// "12 subscribers", or that there are none.
    pub(super) label: String,
    pub(super) visitors_open: bool,
}

pub(super) async fn card(state: &AppState, i18n: &I18n) -> Result<SubscribersCard, AppError> {
    let (members, visitors) = SubscriberRepository::counts(&state.pool).await?;
    let total = usize::try_from(members + visitors).unwrap_or(0);
    let label = if total == 0 {
        i18n.t("notifications.subscribers_none").to_string()
    } else {
        i18n.plural("notifications.subscribers_count", total)
    };
    Ok(SubscribersCard {
        has_any: total > 0,
        label,
        visitors_open: SubscriptionService::visitors_open(&state.pool).await?,
    })
}

/// One subscriber of the list.
struct SubscriberLine {
    id: i64,
    /// The member's name, or the visitor's address.
    name: String,
    /// Who they are, what they receive and since when, as one line.
    sub: String,
    /// The address the removal question names.
    address: String,
    last: LastSend,
}

#[derive(Template)]
#[template(path = "admin/subscribers.html")]
struct SubscribersTemplate {
    frame: Frame,
    lines: Vec<SubscriberLine>,
    removed: bool,
    /// Visitors are subscribed but cannot receive: subscriptions are closed
    /// to them, or the page is not open to them.
    visitors_paused: bool,
    i18n: I18n,
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    done: Option<String>,
}

pub async fn list(
    RequireAdmin(user): RequireAdmin,
    State(state): State<AppState>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
    Query(query): Query<ListQuery>,
) -> Result<Response, AppError> {
    let rows = SubscriberRepository::list_for_admin(&state.pool).await?;
    let has_visitors = rows.iter().any(|row| row.user_id.is_none());
    let open = SubscriptionService::visitors_open(&state.pool).await?;
    let frame = Frame::load(&state.pool, Some(&user), csrf_token, &i18n).await?;
    render(&SubscribersTemplate {
        frame,
        lines: rows.iter().map(|row| line_of(row, &i18n)).collect(),
        removed: query.done.as_deref() == Some("removed"),
        visitors_paused: has_visitors && !(open && state.is_public_mode()),
        i18n,
    })
}

fn line_of(row: &SubscriberRow, i18n: &I18n) -> SubscriberLine {
    let (name, who) = match &row.name {
        Some(name) => (
            name.clone(),
            format!("{}{SEPARATOR}{}", i18n.t("subscribers.member"), row.email),
        ),
        None => (row.email.clone(), i18n.t("subscribers.visitor").to_string()),
    };
    let date = i18n.format_date_long(&clock::local_date(&row.created_at));
    let since = i18n.tf("subscribers.since", &[("date", &date)]);
    let choices = choice_words(row.choices, i18n);
    SubscriberLine {
        id: row.id,
        name,
        sub: format!("{who}{SEPARATOR}{choices}{SEPARATOR}{since}"),
        address: row.email.clone(),
        last: last_of(row, i18n),
    }
}

fn last_of(row: &SubscriberRow, i18n: &I18n) -> LastSend {
    match (row.last_status.as_deref(), &row.last_at) {
        (Some(status), Some(at)) => {
            outcome_line(status == "sent", at, row.last_failure.as_deref(), i18n)
        }
        _ => LastSend::Never,
    }
}

/// Ends a subscription from the list.
pub async fn remove(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let subscriber = SubscriberRepository::find(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let address = address_of(&state.pool, &subscriber).await?;
    if !SubscriberRepository::delete(&state.pool, id).await? {
        return Err(AppError::NotFound);
    }
    NewJournalEntry::new(JournalKind::SubscriberRemoved)
        .by(&admin)
        .about(address)
        .record(&state.pool)
        .await;
    // Never the address in the URL: the page says it generically.
    Ok(Redirect::to("/admin/notifications/subscribers?done=removed").into_response())
}

#[derive(Deserialize)]
pub struct VisitorsForm {
    open: Option<String>,
}

/// Opens or closes subscriptions to visitors. Closing keeps the ones made,
/// which receive nothing until it is opened again.
pub async fn set_visitors(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    HtmlForm(form): HtmlForm<VisitorsForm>,
) -> Result<Response, AppError> {
    let open = form.open.is_some();
    let before = SubscriptionService::visitors_open(&state.pool).await?;
    SubscriptionService::set_visitors_open(&state.pool, open).await?;
    if open != before {
        NewJournalEntry::new(JournalKind::VisitorSubscriptionsChanged)
            .by(&admin)
            .with_detail(if open { "on" } else { "off" })
            .record(&state.pool)
            .await;
    }
    let notice = if open {
        "visitors_open"
    } else {
        "visitors_closed"
    };
    Ok(Redirect::to(&format!("/admin/notifications?notice={notice}")).into_response())
}
