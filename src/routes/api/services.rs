//! Services, as the API reads them.

use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;

use super::caller::ApiCaller;
use crate::clock;
use crate::db::DbPool;
use crate::error::AppError;
use crate::models::Service;
use crate::repositories::ServiceRepository;

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
    let service = ServiceRepository::find_by_id(&pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(ServiceBody::from(service)))
}
