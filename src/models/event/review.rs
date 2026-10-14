//! The review of a resolved incident: what happened, why, and what changes
//! so it does not come back.

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventReview {
    /// Markdown as its author wrote it, rendered when shown.
    pub body: String,
    pub author_name: String,
    pub published_at: DateTime<Utc>,
}
