//! The bell of the masthead and the page of every notice, for publishers and
//! administrators.

use askama::Template;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;

use crate::db::DbPool;
use crate::error::AppError;
use crate::i18n::{I18n, Locale};
use crate::middleware::{CsrfToken, RequirePublisher};
use crate::models::{MemberNotice, NoticeKind, SEPARATOR, User};
use crate::repositories::MemberNoticeRepository;
use crate::routes::{Frame, render};
use crate::state::AppState;

/// How many notices the bell's menu lists.
const BELL_ROWS: i64 = 6;
/// How many the page lists: about a month of them, after which they go.
const PAGE_ROWS: i64 = 100;

/// The bell as the masthead draws it.
pub struct Bell {
    pub unread: i64,
    /// What a screen reader says of the bell.
    pub label: String,
    /// What it says once the notices are seen.
    pub seen_label: String,
    pub rows: Vec<NoticeRow>,
}

/// A notice as a line of the list.
pub struct NoticeRow {
    pub title: String,
    pub text: String,
    pub href: String,
    pub tone: &'static str,
    pub unread: bool,
    pub ago: String,
}

/// The bell of a member who publishes; readers have none.
pub async fn bell(pool: &DbPool, user: &User, i18n: &I18n) -> Result<Option<Bell>, AppError> {
    if !user.role.can_publish() {
        return Ok(None);
    }
    let unread = MemberNoticeRepository::unread_count(pool, user.id).await?;
    let notices = MemberNoticeRepository::list_for(pool, user.id, BELL_ROWS).await?;
    let title = i18n.t("notices.title");
    let label = if unread > 0 {
        let count = i18n.plural("notices.unread", usize::try_from(unread).unwrap_or(0));
        format!("{title}, {count}")
    } else {
        title.to_string()
    };
    Ok(Some(Bell {
        unread,
        label,
        seen_label: title.to_string(),
        rows: notices.iter().map(|notice| row(notice, i18n)).collect(),
    }))
}

fn row(notice: &MemberNotice, i18n: &I18n) -> NoticeRow {
    let (title, text, href, tone) = match notice.kind {
        NoticeKind::UpdateOverdue => overdue(notice, i18n),
        NoticeKind::OutageDetected => (
            i18n.tf(
                "notices.outage_detected",
                &[(
                    "service",
                    notice.service_name.as_deref().unwrap_or_default(),
                )],
            ),
            i18n.t("notices.outage_detected_text").to_string(),
            format!(
                "/events/new?service={}",
                notice.service_id.unwrap_or_default()
            ),
            "crit",
        ),
        NoticeKind::DeliveryFailed => (
            i18n.tf(
                "notices.delivery_failed",
                &[(
                    "channel",
                    notice.channel_name.as_deref().unwrap_or_default(),
                )],
            ),
            i18n.t("notices.delivery_failed_text").to_string(),
            "/admin/notifications".to_string(),
            "minor",
        ),
    };
    NoticeRow {
        title,
        text,
        href,
        tone,
        unread: notice.is_unread(),
        ago: i18n.format_ago(&notice.created_at),
    }
}

fn overdue(notice: &MemberNotice, i18n: &I18n) -> (String, String, String, &'static str) {
    let incident = notice.event_title.clone().unwrap_or_default();
    let text = match notice.due_at {
        Some(due) => {
            let due = i18n.tf("notices.due", &[("when", &i18n.format_due(&due))]);
            format!("{incident}{SEPARATOR}{due}")
        }
        None => incident,
    };
    (
        i18n.t("notices.update_overdue").to_string(),
        text,
        format!("/events/{}", notice.event_id.unwrap_or_default()),
        "ink",
    )
}

#[derive(Template)]
#[template(path = "notices/index.html")]
struct NoticesTemplate {
    frame: Frame,
    rows: Vec<NoticeRow>,
    i18n: I18n,
}

pub async fn index(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    csrf_token: CsrfToken,
    Locale(i18n): Locale,
) -> Result<Response, AppError> {
    let notices = MemberNoticeRepository::list_for(&state.pool, user.id, PAGE_ROWS).await?;
    let rows = notices.iter().map(|notice| row(notice, &i18n)).collect();
    let frame = Frame::load(&state.pool, Some(&user), csrf_token.0, &i18n).await?;
    render(&NoticesTemplate { frame, rows, i18n })
}

/// The bell was opened: what it lists is seen.
pub async fn seen(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    MemberNoticeRepository::mark_all_read(&state.pool, user.id, Utc::now()).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
