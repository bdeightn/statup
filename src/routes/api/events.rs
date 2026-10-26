//! Events, as the API reads them: incidents, maintenances and announcements.

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::caller::ApiCaller;
use crate::clock;
use crate::db::DbPool;
use crate::error::AppError;
use crate::models::{
    Category, Event, EventFilters, EventSummary, EventUpdateWithAuthor, EventWithServices, Kind,
    Lifecycle, LifecycleGroup, NAME_SEPARATOR, Scope, Service, Severity, html_to_text,
};
use crate::repositories::EventRepository;

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 100;

#[derive(Deserialize)]
pub(super) struct ListQuery {
    /// `true` for open events, `false` for closed ones, both when absent.
    open: Option<bool>,
    kind: Option<Kind>,
    limit: Option<i64>,
    offset: Option<i64>,
}

impl ListQuery {
    /// A page out of range is clamped, never refused.
    fn filters(&self) -> EventFilters {
        EventFilters {
            kind: self.kind,
            lifecycle_group: self.open.map(|open| {
                if open {
                    LifecycleGroup::Active
                } else {
                    LifecycleGroup::Closed
                }
            }),
            limit: self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
            offset: self.offset.unwrap_or(0).max(0),
            ..Default::default()
        }
    }
}

#[derive(Serialize)]
pub(super) struct EventList {
    events: Vec<EventBody>,
    /// Events matching the filters, whatever the page.
    total: i64,
}

#[derive(Serialize)]
pub(super) struct EventDetail {
    #[serde(flatten)]
    event: EventBody,
    updates: Vec<UpdateBody>,
}

/// An event in the API. Its own shape, so the contract does not move with
/// the database.
#[derive(Serialize)]
pub(super) struct EventBody {
    id: i64,
    kind: &'static str,
    severity: Option<&'static str>,
    category: Option<&'static str>,
    /// The lifecycle step, none for an announcement.
    state: Option<&'static str>,
    title: String,
    /// The Markdown as written.
    description: String,
    services: Vec<ServiceRef>,
    planned_start: Option<String>,
    planned_end: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    next_update_at: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
pub(super) struct ServiceRef {
    id: i64,
    name: String,
}

#[derive(Serialize)]
pub(super) struct UpdateBody {
    id: i64,
    /// The message as plain text.
    message: String,
    message_html: String,
    author: String,
    created_at: String,
}

fn date(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(clock::rfc3339)
}

/// `Event` and `EventSummary` carry the same fields; only their services
/// differ.
macro_rules! event_body {
    ($source:expr, $services:expr) => {{
        let source = $source;
        EventBody {
            id: source.id,
            kind: source.kind.as_str(),
            severity: source.severity.map(Severity::as_str),
            category: source.category.map(Category::as_str),
            state: source.lifecycle.map(Lifecycle::as_str),
            title: source.title,
            description: source.description,
            services: $services,
            planned_start: date(source.planned_start),
            planned_end: date(source.planned_end),
            started_at: date(source.started_at),
            ended_at: date(source.ended_at),
            next_update_at: date(source.next_update_at),
            created_at: clock::rfc3339(source.created_at),
            updated_at: clock::rfc3339(source.updated_at),
        }
    }};
}

impl EventBody {
    fn of_event(event: Event, services: Vec<ServiceRef>) -> Self {
        event_body!(event, services)
    }
}

impl From<EventSummary> for EventBody {
    fn from(summary: EventSummary) -> Self {
        // The names and the ids are joined in the same order.
        let services = summary
            .service_ids
            .split(',')
            .zip(summary.service_names.split(NAME_SEPARATOR))
            .filter_map(|(id, name)| {
                Some(ServiceRef {
                    id: id.parse().ok()?,
                    name: name.to_string(),
                })
            })
            .collect();
        event_body!(summary, services)
    }
}

impl From<Service> for ServiceRef {
    fn from(service: Service) -> Self {
        Self {
            id: service.id,
            name: service.name,
        }
    }
}

impl From<EventUpdateWithAuthor> for UpdateBody {
    fn from(update: EventUpdateWithAuthor) -> Self {
        Self {
            id: update.id,
            message: html_to_text(&update.message),
            message_html: update.message,
            author: update.author_name,
            created_at: clock::rfc3339(update.created_at),
        }
    }
}

/// Events newest first, a page at a time. A key sees what an editor sees.
pub(super) async fn list(
    _caller: ApiCaller,
    State(pool): State<DbPool>,
    Query(query): Query<ListQuery>,
) -> Result<Json<EventList>, AppError> {
    let filters = query.filters();
    let events = EventRepository::list_page(&pool, &filters, &Scope::All).await?;
    let total = EventRepository::count_filtered(&pool, &filters, &Scope::All).await?;
    Ok(Json(EventList {
        events: events.into_iter().map(EventBody::from).collect(),
        total,
    }))
}

pub(super) async fn show(
    _caller: ApiCaller,
    State(pool): State<DbPool>,
    Path(id): Path<i64>,
) -> Result<Json<EventDetail>, AppError> {
    let EventWithServices { event, services } =
        EventRepository::find_with_services(&pool, id, &Scope::All)
            .await?
            .ok_or(AppError::NotFound)?;
    let updates = EventRepository::list_updates(&pool, id).await?;
    let services = services.into_iter().map(ServiceRef::from).collect();
    Ok(Json(EventDetail {
        event: EventBody::of_event(event, services),
        updates: updates.into_iter().map(UpdateBody::from).collect(),
    }))
}
