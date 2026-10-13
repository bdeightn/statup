//! What the app tells a publisher or an administrator under the bell of the
//! masthead.

use chrono::{DateTime, Utc};

/// What happened, which decides the words and the page a notice leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum NoticeKind {
    /// The update a member announced is late.
    UpdateOverdue,
    /// The checks found a service down.
    OutageDetected,
    /// A message to a destination could not be sent.
    DeliveryFailed,
}

/// What a notice is about; only the one its kind needs is set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoticeSubject {
    pub event_id: Option<i64>,
    pub service_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub due_at: Option<DateTime<Utc>>,
}

/// A notice as the bell lists it, with the names of what it is about.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MemberNotice {
    pub id: i64,
    pub kind: NoticeKind,
    pub event_id: Option<i64>,
    pub service_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub due_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
    pub event_title: Option<String>,
    pub service_name: Option<String>,
    pub channel_name: Option<String>,
}

impl MemberNotice {
    pub fn is_unread(&self) -> bool {
        self.read_at.is_none()
    }
}
