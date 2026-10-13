//! The reminder of an overdue incident update: a notice under the bell of
//! the member who announced it and, with a mail server, an email.

use chrono::{DateTime, Duration, Utc};

use super::deliver::page_address;
use super::{Notifier, ReminderEmail};
use crate::db::DbPool;
use crate::error::AppError;
use crate::i18n::I18n;
use crate::models::{DueReminder, NoticeKind, NoticeSubject, split_duration};
use crate::repositories::{EventRepository, MemberNoticeRepository};

/// Time left to a member who is still writing their update.
const MARGIN: Duration = Duration::minutes(5);

/// Reminds the announcer of each update overdue by more than [`MARGIN`],
/// once per announced time. Returns how many were reminded.
pub async fn remind_overdue(
    pool: &DbPool,
    notifier: &Notifier,
    public_url: Option<&str>,
    now: DateTime<Utc>,
) -> Result<usize, AppError> {
    let due = EventRepository::due_reminders(pool, now - MARGIN).await?;
    if due.is_empty() {
        return Ok(0);
    }
    let page = page_address(pool, public_url).await?;
    let mut reminded = 0;
    for reminder in &due {
        // Noted first: a reminder is better missed than sent twice.
        if !EventRepository::mark_reminded(pool, reminder.event_id, reminder.next_update_at).await?
        {
            continue;
        }
        let subject = NoticeSubject {
            event_id: Some(reminder.event_id),
            due_at: Some(reminder.next_update_at),
            ..NoticeSubject::default()
        };
        let kind = NoticeKind::UpdateOverdue;
        if let Err(e) =
            MemberNoticeRepository::add_for_member(pool, reminder.user_id, kind, subject).await
        {
            tracing::warn!(error = %e, event_id = reminder.event_id, "Overdue update notice not stored");
        }
        if notifier.mail_ready() {
            email(notifier, reminder, page.as_deref(), now).await;
        }
        reminded += 1;
    }
    Ok(reminded)
}

async fn email(
    notifier: &Notifier,
    reminder: &DueReminder,
    page: Option<&str>,
    now: DateTime<Utc>,
) {
    let i18n = I18n::new(reminder.preferred_locale.as_deref().unwrap_or_default());
    let due = i18n.format_due(&reminder.next_update_at);
    let late = i18n.format_duration(&split_duration(now - reminder.next_update_at));
    let url = page.map(|page| format!("{page}/events/{}", reminder.event_id));
    let instance = crate::brand_name();
    let mail = ReminderEmail {
        to: &reminder.email,
        title: &reminder.title,
        due: &due,
        late: &late,
        url: url.as_deref(),
        instance: &instance,
        i18n: &i18n,
    };
    if let Err(failure) = notifier.send_update_reminder(&mail).await {
        tracing::warn!(event_id = reminder.event_id, failure = %failure.code(), "Update reminder email not sent");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CreateEventInput, Kind, NextUpdate, Role, Severity};
    use crate::repositories::UserRepository;
    use crate::test_helpers::test_pool;

    fn notifier() -> Notifier {
        Notifier::new(std::time::Duration::from_secs(1), None).unwrap()
    }

    /// An incident whose update, announced by a fresh member, was due
    /// `minutes_ago`. Returns the member and the incident.
    async fn announced(pool: &DbPool, minutes_ago: i64) -> (i64, i64) {
        let member = UserRepository::create(pool, "ed@example.com", "hash", "Ed", Role::Publisher)
            .await
            .unwrap();
        let input = CreateEventInput {
            kind: Kind::Incident,
            severity: Some(Severity::Critical),
            planned: false,
            category: None,
            title: "Mail is slow".to_string(),
            description: String::new(),
            planned_start: None,
            planned_end: None,
            started_at: None,
            opening_step: None,
            keeps_services_up: false,
            service_ids: Vec::new(),
            follows_event_id: None,
            author_id: member.id,
            next_update: Some(NextUpdate {
                at: Utc::now() - Duration::minutes(minutes_ago),
                every_minutes: None,
            }),
        };
        let event = EventRepository::create(pool, &input).await.unwrap();
        (member.id, event.id)
    }

    #[tokio::test]
    async fn an_overdue_update_leaves_one_notice_for_its_announcer() {
        let pool = test_pool().await;
        let (member, event) = announced(&pool, 10).await;

        let first = remind_overdue(&pool, &notifier(), None, Utc::now())
            .await
            .unwrap();
        let again = remind_overdue(&pool, &notifier(), None, Utc::now())
            .await
            .unwrap();

        assert_eq!((first, again), (1, 0));
        let notices = MemberNoticeRepository::list_for(&pool, member, 10)
            .await
            .unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].kind, NoticeKind::UpdateOverdue);
        assert_eq!(notices[0].event_id, Some(event));
        assert_eq!(notices[0].event_title.as_deref(), Some("Mail is slow"));
    }

    #[tokio::test]
    async fn an_update_within_its_margin_is_left_alone() {
        let pool = test_pool().await;
        let (member, _) = announced(&pool, 2).await;

        let reminded = remind_overdue(&pool, &notifier(), None, Utc::now())
            .await
            .unwrap();

        assert_eq!(reminded, 0);
        let count = MemberNoticeRepository::unread_count(&pool, member)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
