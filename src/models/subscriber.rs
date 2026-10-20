//! The people told by email about what the page publishes.

use chrono::{DateTime, Utc};

/// The kinds of events a subscriber receives, one box each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct Choices {
    #[sqlx(rename = "on_incidents")]
    pub incidents: bool,
    #[sqlx(rename = "on_maintenances")]
    pub maintenances: bool,
    #[sqlx(rename = "on_publications")]
    pub publications: bool,
}

/// A member, at the address of their account, or a visitor, at the address
/// they confirmed. Not `Debug`: it holds the unsubscribe secret in clear.
#[derive(Clone, sqlx::FromRow)]
pub struct Subscriber {
    pub id: i64,
    pub user_id: Option<i64>,
    pub email: Option<String>,
    /// The language of the page when they subscribed.
    pub locale: String,
    #[sqlx(flatten)]
    pub choices: Choices,
    pub unsubscribe_secret: String,
    /// The latest consent.
    pub confirmed_at: DateTime<Utc>,
    /// The first consent.
    pub created_at: DateTime<Utc>,
}

/// A visitor's subscription until they open the link sent to the address.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SubscriptionRequest {
    pub id: i64,
    pub email: String,
    pub secret_hash: String,
    pub locale: String,
    #[sqlx(flatten)]
    pub choices: Choices,
}
