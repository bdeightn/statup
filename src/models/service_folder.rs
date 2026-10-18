//! A folder of services, with its own choice of who sees them.

use super::Visibility;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ServiceFolder {
    pub id: i64,
    pub name: String,
    pub position: i64,
    pub visibility: Visibility,
    /// Filled by the listing query.
    pub service_count: i64,
}
