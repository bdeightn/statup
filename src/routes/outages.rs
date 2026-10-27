//! The outages the checks or a monitoring tool found on a service, as its
//! side panel lists them.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::HeaderValue;
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;

use super::journal::finding_words;
use super::services::service_panel;
use crate::clock;
use crate::db::DbPool;
use crate::error::AppError;
use crate::i18n::{I18n, Locale};
use crate::middleware::headers::is_htmx;
use crate::middleware::{CsrfToken, HtmlForm, RequirePublisher, Viewer};
use crate::models::{JournalKind, Scope, User, split_duration};
use crate::modules::services::{MIN_DETECTED_OUTAGE, SHORT_DAYS};
use crate::repositories::{
    ErasedOutage, NewJournalEntry, Outage, OutageCause, OutageRepository, ServiceRepository,
};
use crate::state::AppState;

/// How far apart two outages may begin and still have fallen together, as
/// the services of one server do when it goes down.
const FALLEN_TOGETHER: TimeDelta = TimeDelta::minutes(5);

/// One outage as the panel shows it, cut to what the viewer may read.
pub struct OutageLine {
    pub id: i64,
    /// "Du 26 oct. à 23:56 au 27 oct. à 08:42".
    pub when: String,
    /// "8 h 46 min".
    pub length: String,
    /// What the checks found, for members.
    pub reason: Option<String>,
    /// The Journal around the outage, for administrators.
    pub journal: Option<String>,
    /// The days of the strip it covers, "2026-10-26 2026-10-27": a click on
    /// one of them shows it.
    pub days: String,
    /// Covers the day the panel opened on.
    pub shown: bool,
    /// Ended, and the viewer may erase it.
    pub erasable: bool,
    /// Other services the viewer sees whose outage began with this one.
    pub fallen_with: usize,
}

/// Who reads the panel, and the day it opened on.
pub struct PanelView<'a> {
    pub user: Option<&'a User>,
    pub scope: &'a Scope,
    pub day: Option<NaiveDate>,
}

/// The outages that coloured the days of a service's strip, newest first.
pub async fn panel_outages(
    pool: &DbPool,
    service_id: i64,
    service_name: &str,
    view: &PanelView<'_>,
    i18n: &I18n,
) -> Result<Vec<OutageLine>, AppError> {
    let now = Utc::now();
    let since = now - chrono::Duration::days(SHORT_DAYS);
    let outages =
        OutageRepository::recent(pool, service_id, since, MIN_DETECTED_OUTAGE, now).await?;
    let first_day = clock::local_date(&since);
    let may_erase = view.user.is_some_and(|u| u.role.can_publish());
    let fallen = if may_erase {
        fallen_with(pool, service_id, since, view.scope).await?
    } else {
        HashMap::new()
    };
    Ok(outages
        .iter()
        .map(|outage| {
            let days = days_covered(outage, first_day, now);
            OutageLine {
                shown: view.day.is_some_and(|day| days.contains(&day)),
                erasable: may_erase && outage.end.is_some(),
                fallen_with: fallen.get(&outage.id).copied().unwrap_or(0),
                days: days
                    .iter()
                    .map(|day| day.format("%Y-%m-%d").to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
                ..line(outage, service_name, view.user, now, i18n)
            }
        })
        .collect())
}

/// For each outage of the service, how many services the viewer sees
/// fell with it.
async fn fallen_with(
    pool: &DbPool,
    service_id: i64,
    since: DateTime<Utc>,
    scope: &Scope,
) -> Result<HashMap<i64, usize>, AppError> {
    let mut counts = HashMap::new();
    for (outage_id, other) in
        OutageRepository::fallen_together(pool, service_id, since, FALLEN_TOGETHER).await?
    {
        if scope.sees_service(other) {
            *counts.entry(outage_id).or_insert(0) += 1;
        }
    }
    Ok(counts)
}

/// The local days an outage spans, from `first_day` on.
fn days_covered(outage: &Outage, first_day: NaiveDate, now: DateTime<Utc>) -> Vec<NaiveDate> {
    let last = clock::local_date(&outage.end.unwrap_or(now));
    clock::local_date(&outage.start)
        .max(first_day)
        .iter_days()
        .take_while(|day| *day <= last)
        .collect()
}

fn line(
    outage: &Outage,
    service_name: &str,
    user: Option<&User>,
    now: DateTime<Utc>,
    i18n: &I18n,
) -> OutageLine {
    let end = outage.end.unwrap_or(now);
    OutageLine {
        id: outage.id,
        when: when(outage, i18n),
        length: i18n.format_duration(&split_duration(end - outage.start)),
        reason: user.and_then(|_| reason(&outage.cause, i18n)),
        journal: user
            .filter(|u| u.role.can_admin())
            .map(|_| journal_link(outage.start, end, service_name)),
        days: String::new(),
        shown: false,
        erasable: false,
        fallen_with: 0,
    }
}

/// "Depuis le 27 oct. à 08:12", "27 oct., de 03:00 à 03:40" or "Du 26
/// oct. à 23:56 au 27 oct. à 08:42".
fn when(outage: &Outage, i18n: &I18n) -> String {
    let Some(end) = outage.end else {
        let start = i18n.format_datetime(&outage.start);
        return i18n.tf("services.outage_since", &[("start", &start)]);
    };
    let day = clock::local_date(&outage.start);
    if day == clock::local_date(&end) {
        return i18n.tf(
            "services.outage_within_day",
            &[
                ("date", &i18n.format_date_short(&day)),
                ("start", &i18n.format_time(&outage.start)),
                ("end", &i18n.format_time(&end)),
            ],
        );
    }
    i18n.tf(
        "services.outage_across_days",
        &[
            ("start", &i18n.format_datetime(&outage.start)),
            ("end", &i18n.format_datetime(&end)),
        ],
    )
}

/// What found the outage, opening its line: "Aucun serveur de noms ne
/// répond", "Signalée par un outil de supervision".
fn reason(cause: &OutageCause, i18n: &I18n) -> Option<String> {
    let words = match cause {
        OutageCause::Check(words) => words.as_deref().and_then(|w| finding_words(w, i18n))?,
        OutageCause::Report => i18n.t("services.outage_reported").to_string(),
    };
    let mut letters = words.chars();
    let first = letters.next()?;
    Some(first.to_uppercase().chain(letters).collect())
}

/// The monitoring lines of the days the outage covered, about this service.
fn journal_link(start: DateTime<Utc>, end: DateTime<Utc>, service_name: &str) -> String {
    let day = |at: &DateTime<Utc>| clock::local_date(at).format("%Y-%m-%d").to_string();
    format!(
        "/admin/journal?category=monitoring&from={}&to={}&q={}",
        day(&start),
        day(&end),
        utf8_percent_encode(service_name, NON_ALPHANUMERIC)
    )
}

#[derive(Deserialize)]
pub struct EraseForm {
    together: Option<String>,
}

/// "It was not an outage": an editor erases an ended outage, and with
/// `together` the ones of the services they see that fell with it. The
/// panel is drawn again and the page behind it learns it is stale.
pub async fn erase(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    Path((id, outage_id)): Path<(i64, i64)>,
    csrf_token: CsrfToken,
    headers: axum::http::HeaderMap,
    Locale(i18n): Locale,
    HtmlForm(form): HtmlForm<EraseForm>,
) -> Result<Response, AppError> {
    let viewer = Viewer::of(&state.pool, Some(user)).await?;
    if !viewer.scope.sees_service(id) {
        return Err(AppError::NotFound);
    }
    let others = if form.together.is_some() {
        visible_others(&state.pool, id, &viewer.scope).await?
    } else {
        Vec::new()
    };
    let erased =
        OutageRepository::erase(&state.pool, id, outage_id, &others, FALLEN_TOGETHER).await?;
    if erased.is_empty() {
        return Err(AppError::NotFound);
    }
    record_erased(&state.pool, &viewer, &erased).await;
    if !is_htmx(&headers) {
        return Ok(Redirect::to("/").into_response());
    }
    let mut response = service_panel(&state, &viewer, id, None, csrf_token.0, i18n).await?;
    response
        .headers_mut()
        .insert("HX-Trigger", HeaderValue::from_static("event-updated"));
    Ok(response)
}

async fn visible_others(pool: &DbPool, id: i64, scope: &Scope) -> Result<Vec<i64>, AppError> {
    let since = Utc::now() - chrono::Duration::days(SHORT_DAYS);
    let mut others: Vec<i64> = OutageRepository::fallen_together(pool, id, since, FALLEN_TOGETHER)
        .await?
        .into_iter()
        .map(|(_, other)| other)
        .filter(|&other| scope.sees_service(other))
        .collect();
    others.sort_unstable();
    others.dedup();
    Ok(others)
}

/// One line per outage, under its own service: each service's Journal
/// link finds its own. The others name the service they were erased from.
async fn record_erased(pool: &DbPool, viewer: &Viewer, erased: &[ErasedOutage]) {
    let Some(user) = viewer.user.as_ref() else {
        return;
    };
    let mut origin: Option<String> = None;
    for outage in erased {
        let Ok(Some(service)) = ServiceRepository::find_by_id(pool, outage.service_id).await else {
            continue;
        };
        let span = format!(
            "{} · {}",
            clock::rfc3339(outage.start),
            clock::rfc3339(outage.end)
        );
        let detail = origin
            .as_ref()
            .map_or_else(|| span.clone(), |origin| format!("{span} · {origin}"));
        origin.get_or_insert_with(|| service.name.clone());
        NewJournalEntry::new(JournalKind::OutageErased)
            .by(user)
            .about(service.name)
            .with_detail(detail)
            .record(pool)
            .await;
    }
}
