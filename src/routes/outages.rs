//! The outages the checks or a monitoring tool found on a service, as its
//! side panel lists them.

use std::collections::{HashMap, HashSet};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use serde::Deserialize;

use super::journal::finding_words;
use super::page::query_pairs;
use super::services::service_panel;
use crate::clock;
use crate::db::DbPool;
use crate::error::AppError;
use crate::i18n::{I18n, Locale};
use crate::middleware::headers::is_htmx;
use crate::middleware::{CsrfToken, HtmlForm, RequirePublisher, Viewer};
use crate::models::{JournalCategory, JournalKind, User, split_duration};
use crate::modules::services::{lasted_long_enough, short_strip_start};
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
    /// The days of the strip it covers: a click on one of them shows it.
    pub days: Vec<NaiveDate>,
    /// Covers the day the panel opened on.
    pub shown: bool,
    /// Ended, and the viewer may erase it.
    pub erasable: bool,
    /// Other services the viewer sees that fell with it.
    pub fallen_with: usize,
}

/// The outages that coloured the days of a service's strip, newest first.
pub async fn panel_outages(
    pool: &DbPool,
    service_id: i64,
    service_name: &str,
    viewer: &Viewer,
    day: Option<NaiveDate>,
    i18n: &I18n,
) -> Result<Vec<OutageLine>, AppError> {
    let now = Utc::now();
    let since = short_strip_start();
    let user = viewer.user.as_ref();
    let may_erase = user.is_some_and(|u| u.role.can_publish());
    let fallen = if may_erase {
        fallen_together(pool, service_id, viewer).await?
    } else {
        HashMap::new()
    };
    let outages = OutageRepository::recent(pool, service_id, since).await?;
    Ok(outages
        .iter()
        .filter(|outage| lasted_long_enough(outage.start, outage.end, now))
        .map(|outage| {
            let days = days_covered(outage, clock::local_date(&since), now);
            let end = outage.end.unwrap_or(now);
            OutageLine {
                id: outage.id,
                when: when(outage, i18n),
                length: i18n.format_duration(&split_duration(end - outage.start)),
                reason: user.and_then(|_| reason(&outage.cause, i18n)),
                journal: user
                    .filter(|u| u.role.can_admin())
                    .map(|_| journal_link(outage.start, end, service_name)),
                shown: day.is_some_and(|day| days.contains(&day)),
                erasable: may_erase && outage.end.is_some(),
                fallen_with: fallen.get(&outage.id).map_or(0, FallenWith::services),
                days,
            }
        })
        .collect())
}

/// The ended outages of the services a viewer sees that began with one
/// outage of a service.
#[derive(Default)]
struct FallenWith {
    outages: Vec<i64>,
    services: HashSet<i64>,
}

impl FallenWith {
    fn services(&self) -> usize {
        self.services.len()
    }
}

/// For each recent outage of the service, what fell with it.
async fn fallen_together(
    pool: &DbPool,
    service_id: i64,
    viewer: &Viewer,
) -> Result<HashMap<i64, FallenWith>, AppError> {
    let mut fallen: HashMap<i64, FallenWith> = HashMap::new();
    let rows =
        OutageRepository::fallen_together(pool, service_id, short_strip_start(), FALLEN_TOGETHER)
            .await?;
    for (mine, other, other_service) in rows {
        if viewer.scope.sees_service(other_service) {
            let entry = fallen.entry(mine).or_default();
            entry.outages.push(other);
            entry.services.insert(other_service);
        }
    }
    Ok(fallen)
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
    Some(I18n::capitalized(&words))
}

/// The monitoring lines of the days the outage covered, about this service.
fn journal_link(start: DateTime<Utc>, end: DateTime<Utc>, service_name: &str) -> String {
    let query = query_pairs(&[
        (
            "category",
            Some(JournalCategory::Monitoring.as_str().to_string()),
        ),
        ("from", Some(clock::local_date(&start).to_string())),
        ("to", Some(clock::local_date(&end).to_string())),
        ("q", Some(service_name.to_string())),
    ]);
    format!("/admin/journal?{}", query.trim_end_matches('&'))
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
    headers: HeaderMap,
    Locale(i18n): Locale,
    HtmlForm(form): HtmlForm<EraseForm>,
) -> Result<Response, AppError> {
    let viewer = Viewer::of(&state.pool, Some(user)).await?;
    if !viewer.scope.sees_service(id) {
        return Err(AppError::NotFound);
    }
    let others = if form.together.is_some() {
        fallen_together(&state.pool, id, &viewer)
            .await?
            .remove(&outage_id)
            .unwrap_or_default()
            .outages
    } else {
        Vec::new()
    };
    let erased = OutageRepository::erase(&state.pool, id, outage_id, &others).await?;
    if erased.is_empty() {
        return Err(AppError::NotFound);
    }
    if let Some(user) = viewer.user.as_ref() {
        record_erased(&state.pool, user, &erased).await;
    }
    if !is_htmx(&headers) {
        return Ok(Redirect::to("/").into_response());
    }
    let mut response = service_panel(&state, &viewer, id, None, csrf_token.0, i18n).await?;
    response
        .headers_mut()
        .insert("HX-Trigger", HeaderValue::from_static("event-updated"));
    Ok(response)
}

/// One line per outage, under its own service: each service's Journal
/// link finds its own. The others name the service they were erased from.
async fn record_erased(pool: &DbPool, user: &User, erased: &[ErasedOutage]) {
    let Some((first, others)) = erased.split_first() else {
        return;
    };
    let ids: Vec<i64> = erased.iter().map(|outage| outage.service_id).collect();
    let names = match ServiceRepository::names(pool, &ids).await {
        Ok(names) => names,
        Err(e) => {
            tracing::warn!(error = %e, "Erased outages could not be journaled");
            return;
        }
    };
    let Some(origin) = names.get(&first.service_id) else {
        return;
    };
    record_one(pool, user, first, origin, None).await;
    for outage in others {
        if let Some(name) = names.get(&outage.service_id) {
            record_one(pool, user, outage, name, Some(origin)).await;
        }
    }
}

async fn record_one(
    pool: &DbPool,
    user: &User,
    outage: &ErasedOutage,
    name: &str,
    origin: Option<&String>,
) {
    let mut detail = format!(
        "{} · {}",
        clock::rfc3339(outage.start),
        clock::rfc3339(outage.end)
    );
    if let Some(origin) = origin {
        detail = format!("{detail} · {origin}");
    }
    NewJournalEntry::new(JournalKind::OutageErased)
        .by(user)
        .about(name.to_string())
        .with_detail(detail)
        .record(pool)
        .await;
}
