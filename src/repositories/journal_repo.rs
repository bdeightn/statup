//! Journal lines: written one by one or in batches, and forgotten once
//! older than the retention.

use chrono::{DateTime, Utc};
use sqlx::{QueryBuilder, Sqlite};

use crate::clock;
use crate::db::DbPool;
use crate::models::JournalKind;

/// A line to write; its family and importance follow from its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewJournalEntry {
    pub kind: JournalKind,
    pub actor_id: Option<i64>,
    pub actor_name: Option<String>,
    pub subject: Option<String>,
    pub detail: Option<String>,
    pub ip: Option<String>,
}

impl NewJournalEntry {
    pub fn new(kind: JournalKind) -> Self {
        Self {
            kind,
            actor_id: None,
            actor_name: None,
            subject: None,
            detail: None,
            ip: None,
        }
    }
}

pub struct JournalRepository;

impl JournalRepository {
    pub async fn add(pool: &DbPool, entry: &NewJournalEntry) -> Result<(), sqlx::Error> {
        Self::add_all(pool, std::slice::from_ref(entry)).await
    }

    /// Writes the lines in one statement, in their order.
    pub async fn add_all(pool: &DbPool, entries: &[NewJournalEntry]) -> Result<(), sqlx::Error> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut query = QueryBuilder::<Sqlite>::new(
            "INSERT INTO journal_entries \
             (kind, category, essential, actor_id, actor_name, subject, detail, ip) ",
        );
        query.push_values(entries, |mut row, entry| {
            row.push_bind(entry.kind)
                .push_bind(entry.kind.category())
                .push_bind(entry.kind.is_essential())
                .push_bind(entry.actor_id)
                .push_bind(entry.actor_name.as_deref())
                .push_bind(entry.subject.as_deref())
                .push_bind(entry.detail.as_deref())
                .push_bind(entry.ip.as_deref());
        });
        query.build().execute(pool).await?;
        Ok(())
    }

    /// Forgets the lines written before `before`.
    pub async fn purge(pool: &DbPool, before: DateTime<Utc>) -> Result<u64, sqlx::Error> {
        let done = sqlx::query("DELETE FROM journal_entries WHERE created_at < ?")
            .bind(clock::db(before))
            .execute(pool)
            .await?;
        Ok(done.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;
    use crate::test_helpers::test_pool;

    async fn rows(pool: &DbPool) -> Vec<(String, String, bool, Option<String>)> {
        sqlx::query_as("SELECT kind, category, essential, subject FROM journal_entries ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_line_keeps_the_family_and_importance_of_its_kind() {
        let pool = test_pool().await;
        let mut error = NewJournalEntry::new(JournalKind::AppError);
        error.subject = Some("Database error".into());
        let warning = NewJournalEntry::new(JournalKind::AppWarning);

        JournalRepository::add_all(&pool, &[error, warning])
            .await
            .unwrap();

        assert_eq!(
            rows(&pool).await,
            vec![
                (
                    "app_error".into(),
                    "system".into(),
                    true,
                    Some("Database error".into())
                ),
                ("app_warning".into(), "system".into(), false, None),
            ]
        );
    }

    #[tokio::test]
    async fn the_purge_forgets_only_the_older_lines() {
        let pool = test_pool().await;
        JournalRepository::add(&pool, &NewJournalEntry::new(JournalKind::AppError))
            .await
            .unwrap();
        sqlx::query("UPDATE journal_entries SET created_at = datetime('now', '-100 days')")
            .execute(&pool)
            .await
            .unwrap();
        JournalRepository::add(&pool, &NewJournalEntry::new(JournalKind::AppWarning))
            .await
            .unwrap();

        let forgotten = JournalRepository::purge(&pool, Utc::now() - Duration::days(90))
            .await
            .unwrap();

        assert_eq!(forgotten, 1);
        assert_eq!(rows(&pool).await.len(), 1);
    }
}
