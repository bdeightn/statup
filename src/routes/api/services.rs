//! Services, as the API reads them.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use super::caller::ApiCaller;
use crate::clock;
use crate::db::DbPool;
use crate::error::AppError;
use crate::models::{JournalKind, Service, ServiceStatus};
use crate::repositories::{NewJournalEntry, ServiceRepository};
use crate::services::ServiceService;

#[derive(Serialize)]
pub(super) struct ServiceList {
    services: Vec<ServiceBody>,
}

/// A service in the API. Its own shape, so the contract does not move with
/// the database.
#[derive(Serialize)]
pub(super) struct ServiceBody {
    id: i64,
    name: String,
    slug: String,
    description: Option<String>,
    /// The shown state: the worst of the one set by hand and open events.
    status: &'static str,
    /// The state set by hand, the one a later write changes.
    manual_status: &'static str,
    /// Statup checks the service itself.
    monitored: bool,
    updated_at: String,
}

impl From<Service> for ServiceBody {
    fn from(service: Service) -> Self {
        Self {
            id: service.id,
            name: service.name,
            slug: service.slug,
            description: service.description,
            status: service.status.as_str(),
            manual_status: service.manual_status.as_str(),
            monitored: service.check_kind.is_some(),
            updated_at: clock::rfc3339(service.updated_at),
        }
    }
}

/// Every service, in the chosen order. A key sees what an editor sees.
pub(super) async fn list(
    _caller: ApiCaller,
    State(pool): State<DbPool>,
) -> Result<Json<ServiceList>, AppError> {
    let services = ServiceRepository::list_all(&pool).await?;
    Ok(Json(ServiceList {
        services: services.into_iter().map(ServiceBody::from).collect(),
    }))
}

pub(super) async fn show(
    _caller: ApiCaller,
    State(pool): State<DbPool>,
    Path(id): Path<i64>,
) -> Result<Json<ServiceBody>, AppError> {
    read_back(&pool, id).await
}

/// The fields the API takes on a new service. Anything else is refused: the
/// visibility, folder and icon are not set from here.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NewService {
    /// A missing name is refused by the service rules, as an empty one is.
    #[serde(default)]
    name: String,
    description: Option<String>,
}

/// A field left out keeps its value.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ServiceChanges {
    name: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StatusChange {
    #[serde(default)]
    status: String,
}

async fn read_back(pool: &DbPool, id: i64) -> Result<Json<ServiceBody>, AppError> {
    let service = ServiceRepository::find_by_id(pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(ServiceBody::from(service)))
}

/// A new service is public, outside any folder and not monitored.
pub(super) async fn create(
    caller: ApiCaller,
    State(pool): State<DbPool>,
    Json(input): Json<NewService>,
) -> Result<(StatusCode, Json<ServiceBody>), AppError> {
    let service =
        ServiceService::create(&pool, &input.name, input.description.as_deref(), None, None)
            .await?;
    NewJournalEntry::new(JournalKind::ServiceCreated)
        .by_key(&caller.key)
        .about(&service.name)
        .record(&pool)
        .await;
    Ok((StatusCode::CREATED, Json(ServiceBody::from(service))))
}

/// The icon is not part of the API: it stays as it is.
pub(super) async fn update(
    caller: ApiCaller,
    State(pool): State<DbPool>,
    Path(id): Path<i64>,
    Json(changes): Json<ServiceChanges>,
) -> Result<Json<ServiceBody>, AppError> {
    let service = ServiceRepository::find_by_id(&pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let name = changes.name.as_deref().unwrap_or(&service.name);
    let description = changes
        .description
        .as_deref()
        .or(service.description.as_deref());
    ServiceService::update(
        &pool,
        id,
        name,
        description,
        service.icon_id,
        service.icon_name.as_deref(),
    )
    .await?;
    NewJournalEntry::new(JournalKind::ServiceUpdated)
        .by_key(&caller.key)
        .about(name.trim())
        .record(&pool)
        .await;
    read_back(&pool, id).await
}

pub(super) async fn set_status(
    caller: ApiCaller,
    State(pool): State<DbPool>,
    Path(id): Path<i64>,
    Json(change): Json<StatusChange>,
) -> Result<Json<ServiceBody>, AppError> {
    let status: ServiceStatus = change
        .status
        .parse()
        .map_err(|()| AppError::validation("error.invalid_data"))?;
    let service = ServiceRepository::find_by_id(&pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    ServiceService::set_manual_status(&pool, id, status).await?;
    NewJournalEntry::new(JournalKind::ServiceStatusSet)
        .by_key(&caller.key)
        .about(&service.name)
        .with_detail(status.as_str())
        .record(&pool)
        .await;
    read_back(&pool, id).await
}
