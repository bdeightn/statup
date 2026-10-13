//! Notices for publishers and administrators, written beside the work that
//! causes them and never in its way: a notice that cannot be stored is
//! logged, the outage or the delivery goes on.

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

async fn tell(pool: &DbPool, roles: &[Role], kind: NoticeKind, subject: NoticeSubject) {
    if let Err(e) = MemberNoticeRepository::add_for_roles(pool, roles, kind, subject).await {
        tracing::warn!(error = %e, ?kind, "Member notice not stored");
    }
}
