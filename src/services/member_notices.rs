//! Notices for publishers and administrators, written beside the work that
//! causes them and never in its way: a notice that cannot be stored is
//! logged, the outage or the delivery goes on.

use chrono::{DateTime, Utc};

use crate::db::DbPool;
use crate::models::{NoticeKind, NoticeSubject, Role};
use crate::repositories::MemberNoticeRepository;

const EDITORS: [Role; 2] = [Role::Publisher, Role::Admin];

/// The checks found a service down: every editor hears of it.
pub async fn tell_outage_detected(pool: &DbPool, service_id: i64) {
    let subject = NoticeSubject {
        service_id: Some(service_id),
        ..NoticeSubject::default()
    };
    tell(pool, &EDITORS, NoticeKind::OutageDetected, subject).await;
}

/// A destination refused its messages: the administrators, who set it up,
/// hear of it.
pub async fn tell_delivery_failed(pool: &DbPool, channel_id: i64) {
    let subject = NoticeSubject {
        channel_id: Some(channel_id),
        ..NoticeSubject::default()
    };
    tell(pool, &[Role::Admin], NoticeKind::DeliveryFailed, subject).await;
}

/// The update a member announced is late: they hear of it.
pub async fn tell_update_overdue(
    pool: &DbPool,
    user_id: i64,
    event_id: i64,
    due_at: DateTime<Utc>,
) {
    let subject = NoticeSubject {
        event_id: Some(event_id),
        due_at: Some(due_at),
        ..NoticeSubject::default()
    };
    let kind = NoticeKind::UpdateOverdue;
    if let Err(e) = MemberNoticeRepository::add_for_member(pool, user_id, kind, subject).await {
        tracing::warn!(error = %e, ?kind, "Member notice not stored");
    }
}

async fn tell(pool: &DbPool, roles: &[Role], kind: NoticeKind, subject: NoticeSubject) {
    if let Err(e) = MemberNoticeRepository::add_for_roles(pool, roles, kind, subject).await {
        tracing::warn!(error = %e, ?kind, "Member notice not stored");
    }
}
