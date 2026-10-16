//! Writes the caught lines in batches, and forgets the lines older than
//! the retention once an hour.

use std::sync::atomic::Ordering;
use std::time::Duration;

use chrono::Utc;
use tokio::task::AbortHandle;

use super::{JournalInbox, TARGET};
use crate::db::DbPool;
use crate::repositories::{JournalRepository, NewJournalEntry, SettingsRepository};

/// How many days a line is kept, unless an administrator chose otherwise.
pub const DEFAULT_RETENTION_DAYS: i64 = 90;

/// The setting that holds the retention, in days.
pub const RETENTION_SETTING: &str = "journal_retention_days";

const BATCH: usize = 64;
const PURGE_INTERVAL: Duration = Duration::from_secs(60 * 60);

pub fn spawn_journal_writer(pool: DbPool, inbox: JournalInbox) -> AbortHandle {
    tokio::spawn(run(pool, inbox)).abort_handle()
}

async fn run(pool: DbPool, mut inbox: JournalInbox) {
    let mut purge = tokio::time::interval(PURGE_INTERVAL);
    loop {
        tokio::select! {
            received = inbox.receiver.recv() => {
                let Some(first) = received else { return };
                let mut batch = vec![first];
                while batch.len() < BATCH {
                    match inbox.receiver.try_recv() {
                        Ok(next) => batch.push(next),
                        Err(_) => break,
                    }
                }
                write(&pool, &batch).await;
                report_dropped(&inbox);
            }
            _ = purge.tick() => forget_old(&pool).await,
        }
    }
}

async fn write(pool: &DbPool, batch: &[NewJournalEntry]) {
    if let Err(error) = JournalRepository::add_all(pool, batch).await {
        tracing::warn!(target: TARGET, %error, lines = batch.len(), "Journal lines not saved");
    }
}

fn report_dropped(inbox: &JournalInbox) {
    let dropped = inbox.dropped.swap(0, Ordering::Relaxed);
    if dropped > 0 {
        tracing::warn!(target: TARGET, dropped, "Journal lines dropped: too many at once");
    }
}

async fn forget_old(pool: &DbPool) {
    let days = retention_days(pool).await;
    let before = Utc::now() - chrono::Duration::days(days);
    match JournalRepository::purge(pool, before).await {
        Ok(0) => {}
        Ok(forgotten) => {
            tracing::debug!(target: TARGET, forgotten, days, "Old journal lines forgotten");
        }
        Err(error) => tracing::warn!(target: TARGET, %error, "Old journal lines not forgotten"),
    }
}

/// The retention chosen on the Journal page, or the default.
async fn retention_days(pool: &DbPool) -> i64 {
    SettingsRepository::get(pool, RETENTION_SETTING)
        .await
        .ok()
        .flatten()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_RETENTION_DAYS)
}
