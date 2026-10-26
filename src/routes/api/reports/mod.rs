//! Reports from monitoring tools: a service went down or came back. Each
//! tool speaks its own format; a reader per format turns its message into
//! signals, and every signal takes the path of Statup's own checks, so an
//! outage a tool found is told as one the checks found. No incident is
//! published without someone.

mod alertmanager;
mod kuma;
mod statup;

use std::collections::HashSet;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Map, Value};

use super::caller::{ApiCaller, bearer_key};
use super::error::refusal;
use crate::db::DbPool;
use crate::error::AppError;
use crate::models::{ApiKeyHolder, Happening, JournalKind, Service, excerpt};
use crate::repositories::{EventRepository, NewJournalEntry, ServiceRepository};
use crate::services::{announce, declare_outage, end_outage};

/// Longest service name a Journal line keeps from a report, as long as a
/// service's own.
const MAX_NAME_CHARS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reported {
    Down,
    Up,
}

impl Reported {
    fn as_str(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Up => "up",
        }
    }
}

/// What a tool says about one service, named as the tool names it.
#[derive(Debug, PartialEq, Eq)]
struct Signal {
    service: String,
    state: Reported,
    message: Option<String>,
}

/// A part of a report Statup did not act on, and why.
#[derive(Debug, PartialEq, Eq, Serialize)]
struct Ignored {
    service: Option<String>,
    reason: &'static str,
}

/// A tool's message, read in its own format.
#[derive(Debug, Default, PartialEq, Eq)]
struct Reading {
    signals: Vec<Signal>,
    ignored: Vec<Ignored>,
}

#[derive(Serialize)]
struct Applied {
    service: String,
    state: &'static str,
}

#[derive(Serialize)]
struct Outcome {
    applied: Vec<Applied>,
    ignored: Vec<Ignored>,
}

/// Why a signal names a service Statup does not act on. These leave a
/// line in the Journal: a tool set up wrong is what an administrator needs
/// to see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    UnknownService,
    CheckedByStatup,
    UnderMaintenance,
}

impl Refusal {
    fn code(self) -> &'static str {
        match self {
            Self::UnknownService => "unknown_service",
            Self::CheckedByStatup => "monitored_by_statup",
            Self::UnderMaintenance => "under_maintenance",
        }
    }

    /// The detail of the Journal line, in English as every detail is.
    fn words(self) -> &'static str {
        match self {
            Self::UnknownService => "unknown service",
            Self::CheckedByStatup => "checked by Statup",
            Self::UnderMaintenance => "under maintenance",
        }
    }

    /// What the Statup format answers, so a script sees it failed.
    fn answer(self) -> Response {
        match self {
            Self::UnknownService => refusal(
                StatusCode::NOT_FOUND,
                self.code(),
                "No service has this name or slug.",
            ),
            Self::CheckedByStatup => refusal(
                StatusCode::CONFLICT,
                self.code(),
                "Statup checks this service itself: its own checks decide its state.",
            ),
            Self::UnderMaintenance => refusal(
                StatusCode::CONFLICT,
                self.code(),
                "A maintenance under way covers this service.",
            ),
        }
    }
}

enum Verdict<'s> {
    Apply(&'s Service),
    Unchanged,
    Refused(Refusal),
}

/// Takes a report in any format Statup reads. The key comes in the
/// `Authorization` header or, for a tool that cannot set one, in a `key`
/// field of the body; never in the address.
pub(super) async fn receive(
    State(pool): State<DbPool>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let from_header = match bearer_key(&headers) {
        Some(presented) => Some(ApiCaller::presenting(&pool, presented).await?),
        None => None,
    };
    let mut fields = read_body(&headers, &body)?;
    let from_body = take_key(&mut fields);
    let caller = if let Some(caller) = from_header {
        caller
    } else {
        let presented = from_body.ok_or(AppError::Unauthorized)?;
        ApiCaller::presenting(&pool, &presented).await?
    };
    let body = Value::Object(fields);
    if let Some(reading) = alertmanager::read(&body).or_else(|| kuma::read(&body)) {
        let outcome = Applying::start(&pool, &caller.key, reading.ignored)
            .await?
            .take_all(reading.signals)
            .await?;
        return Ok(Json(outcome).into_response());
    }
    match statup::read(&body) {
        Some(Ok(signal)) => answer_one(&pool, &caller.key, signal).await,
        Some(Err(unreadable)) => Ok(unreadable.answer()),
        None => Ok(refusal(
            StatusCode::BAD_REQUEST,
            "unknown_format",
            "The body is in no format Statup reads.",
        )),
    }
}

/// The fields of the body: a form when the tool says so, else JSON whatever
/// its content type says, since monitoring tools are loose with it. Nothing
/// of the body is ever said back.
fn read_body(headers: &HeaderMap, body: &[u8]) -> Result<Map<String, Value>, AppError> {
    let unreadable = || AppError::validation("error.invalid_data");
    let form = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/x-www-form-urlencoded"));
    if form {
        let pairs: Vec<(String, String)> =
            serde_html_form::from_bytes(body).map_err(|_| unreadable())?;
        return Ok(pairs
            .into_iter()
            .map(|(name, value)| (name, Value::String(value)))
            .collect());
    }
    match serde_json::from_slice(body) {
        Ok(Value::Object(fields)) => Ok(fields),
        _ => Err(unreadable()),
    }
}

/// Takes the key out of the body, so that no reader ever sees it.
fn take_key(fields: &mut Map<String, Value>) -> Option<String> {
    match fields.remove("key")? {
        Value::String(key) => Some(key),
        _ => None,
    }
}

/// The Statup format names one service: a refusal is said with its own
/// status rather than in a list, so a script sees it failed.
async fn answer_one(
    pool: &DbPool,
    key: &ApiKeyHolder,
    signal: Signal,
) -> Result<Response, AppError> {
    let mut applying = Applying::start(pool, key, Vec::new()).await?;
    let refused = applying.take(signal).await?;
    let outcome = applying.finish().await;
    Ok(refused.map_or_else(|| Json(outcome).into_response(), Refusal::answer))
}

/// The service a tool names, by its name or its slug, whatever the case.
fn find<'s>(services: &'s [Service], named: &str) -> Option<&'s Service> {
    let wanted = named.trim().to_lowercase();
    services
        .iter()
        .find(|service| service.name.to_lowercase() == wanted || service.slug == wanted)
}

/// Two reports racing to declare the same outage: the second meets the one
/// open outage a service may have, and changes nothing.
fn already_open(error: &AppError) -> bool {
    matches!(error, AppError::Database(sqlx::Error::Database(db)) if db.is_unique_violation())
}

/// One report being applied: what it may act on and what it changed.
struct Applying<'a> {
    pool: &'a DbPool,
    key: &'a ApiKeyHolder,
    now: DateTime<Utc>,
    services: Vec<Service>,
    under_maintenance: HashSet<i64>,
    /// The services this report already changed: a second signal on one
    /// reads the state the report started from.
    changed: HashSet<i64>,
    changes: Vec<(Happening, i64)>,
    outcome: Outcome,
}

impl<'a> Applying<'a> {
    async fn start(
        pool: &'a DbPool,
        key: &'a ApiKeyHolder,
        ignored: Vec<Ignored>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            pool,
            key,
            now: Utc::now(),
            services: ServiceRepository::list_all(pool).await?,
            under_maintenance: EventRepository::services_under_maintenance(pool)
                .await?
                .into_iter()
                .collect(),
            changed: HashSet::new(),
            changes: Vec::new(),
            outcome: Outcome {
                applied: Vec::new(),
                ignored,
            },
        })
    }

    async fn take_all(mut self, signals: Vec<Signal>) -> Result<Outcome, AppError> {
        for signal in signals {
            self.take(signal).await?;
        }
        Ok(self.finish().await)
    }

    /// Tells what changed as one wave, as a round of the checks does.
    async fn finish(self) -> Outcome {
        announce(self.pool, &self.changes, self.now).await;
        self.outcome
    }

    async fn take(&mut self, signal: Signal) -> Result<Option<Refusal>, AppError> {
        let (service_id, name) = match self.judge(&signal) {
            Verdict::Apply(service) => (service.id, service.name.clone()),
            Verdict::Unchanged => {
                self.ignore(signal.service, "unchanged");
                return Ok(None);
            }
            Verdict::Refused(refusal) => {
                self.refuse(signal.service, refusal).await;
                return Ok(Some(refusal));
            }
        };
        let told = match signal.state {
            Reported::Down => match declare_outage(self.pool, service_id, self.now).await {
                Ok(told) => told,
                Err(e) if already_open(&e) => {
                    self.ignore(signal.service, "unchanged");
                    return Ok(None);
                }
                Err(e) => return Err(e),
            },
            Reported::Up => Some(end_outage(self.pool, service_id, self.now).await?),
        };
        self.changed.insert(service_id);
        self.changes
            .extend(told.map(|happening| (happening, service_id)));
        self.record(&name, &signal).await;
        self.outcome.applied.push(Applied {
            service: name,
            state: signal.state.as_str(),
        });
        Ok(None)
    }

    /// One service, one source: a service Statup checks itself is left to
    /// its checks, which would undo the report the next minute.
    fn judge(&self, signal: &Signal) -> Verdict<'_> {
        let Some(service) = find(&self.services, &signal.service) else {
            return Verdict::Refused(Refusal::UnknownService);
        };
        if service.check_kind.is_some() {
            return Verdict::Refused(Refusal::CheckedByStatup);
        }
        let down = signal.state == Reported::Down;
        if down && self.under_maintenance.contains(&service.id) {
            return Verdict::Refused(Refusal::UnderMaintenance);
        }
        let already = service.detected_status.is_some() == down;
        if already || self.changed.contains(&service.id) {
            return Verdict::Unchanged;
        }
        Verdict::Apply(service)
    }

    async fn record(&self, name: &str, signal: &Signal) {
        let kind = match signal.state {
            Reported::Down => JournalKind::OutageDetected,
            Reported::Up => JournalKind::OutageEnded,
        };
        let mut entry = NewJournalEntry::new(kind).by_key(self.key).about(name);
        if let Some(message) = &signal.message {
            entry = entry.with_detail(message.as_str());
        }
        entry.record(self.pool).await;
    }

    async fn refuse(&mut self, service: String, refusal: Refusal) {
        NewJournalEntry::new(JournalKind::ReportIgnored)
            .by_key(self.key)
            .about(excerpt(service.trim(), MAX_NAME_CHARS))
            .with_detail(refusal.words())
            .record(self.pool)
            .await;
        self.ignore(service, refusal.code());
    }

    fn ignore(&mut self, service: String, reason: &'static str) {
        self.outcome.ignored.push(Ignored {
            service: Some(service),
            reason,
        });
    }
}
