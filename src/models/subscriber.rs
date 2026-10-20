//! The people told by email about what the page publishes.

use chrono::{DateTime, Utc};

use super::{Happening, Lifecycle};

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

/// The next email a subscriber is waiting for, with what it takes to write
/// and address it. Not `Debug`: it holds the unsubscribe secret.
#[derive(Clone, sqlx::FromRow)]
pub struct SubscriberMessage {
    pub id: i64,
    pub subscriber_id: i64,
    pub happening: Happening,
    pub event_id: i64,
    pub update_id: Option<i64>,
    /// The state the event moved to, when it moved.
    pub lifecycle: Option<Lifecycle>,
    pub attempts: i64,
    /// When it happened, which the email tells.
    pub created_at: DateTime<Utc>,
    pub user_id: Option<i64>,
    pub email: Option<String>,
    pub locale: String,
    pub unsubscribe_secret: String,
}
