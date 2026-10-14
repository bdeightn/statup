//! Reviews of resolved incidents: one per incident, published, corrected
//! and removed. Each change dates its event, so the feed entry says it.

use sqlx::{Sqlite, Transaction};

use crate::db::DbPool;
use crate::models::{EventReview, ReviewSaved};

pub struct EventReviewRepository;

impl EventReviewRepository {
    pub async fn find(pool: &DbPool, event_id: i64) -> Result<Option<EventReview>, sqlx::Error> {
        sqlx::query_as::<_, EventReview>(
            "SELECT r.body, r.published_at, u.display_name AS author_name \
             FROM event_reviews r INNER JOIN users u ON u.id = r.author_id \
             WHERE r.event_id = ?",
        )
        .bind(event_id)
        .fetch_optional(pool)
        .await
    }

    /// Publishes the review of `event_id`, or corrects the one it has: a
    /// correction keeps who published it and when. Of two members saving at
    /// once, one publishes and the other corrects.
    pub async fn save(
        pool: &DbPool,
        event_id: i64,
        body: &str,
        author_id: i64,
    ) -> Result<ReviewSaved, sqlx::Error> {
        let mut tx = pool.begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO event_reviews (event_id, body, author_id) VALUES (?, ?, ?) \
             ON CONFLICT (event_id) DO NOTHING",
        )
        .bind(event_id)
        .bind(body)
        .bind(author_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted == 0 {
            sqlx::query("UPDATE event_reviews SET body = ? WHERE event_id = ?")
                .bind(body)
                .bind(event_id)
                .execute(&mut *tx)
                .await?;
        }
        touch_event(&mut tx, event_id).await?;
        tx.commit().await?;
        Ok(if inserted == 0 {
            ReviewSaved::Corrected
        } else {
            ReviewSaved::Published
        })
    }

    pub async fn delete(pool: &DbPool, event_id: i64) -> Result<(), sqlx::Error> {
        let mut tx = pool.begin().await?;
        let removed = sqlx::query("DELETE FROM event_reviews WHERE event_id = ?")
            .bind(event_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if removed > 0 {
            touch_event(&mut tx, event_id).await?;
        }
        tx.commit().await
    }
}

async fn touch_event(tx: &mut Transaction<'_, Sqlite>, event_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE events SET updated_at = datetime('now') WHERE id = ?")
        .bind(event_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CreateEventInput, Kind, Role, Severity};
    use crate::repositories::{EventRepository, UserRepository};
    use crate::test_helpers::test_pool;

    async fn member(pool: &DbPool, email: &str, name: &str) -> i64 {
        UserRepository::create(pool, email, "hash", name, Role::Publisher)
            .await
            .unwrap()
            .id
    }

    async fn incident(pool: &DbPool, author_id: i64) -> i64 {
        let input = CreateEventInput {
            kind: Kind::Incident,
            severity: Some(Severity::Critical),
            planned: false,
            category: None,
            title: "Payroll down".to_string(),
            description: String::new(),
            planned_start: None,
            planned_end: None,
            started_at: None,
            opening_step: None,
            keeps_services_up: false,
            service_ids: Vec::new(),
            follows_event_id: None,
            author_id,
            next_update: None,
        };
        EventRepository::create(pool, &input).await.unwrap().id
    }

    const LONG_AGO: &str = "2026-10-01 09:00:00";

    async fn date_events_long_ago(pool: &DbPool) {
        sqlx::query("UPDATE events SET updated_at = ?")
            .bind(LONG_AGO)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn feed_entry(pool: &DbPool, id: i64) -> crate::models::EventSummary {
        EventRepository::list_for_feed(pool, 10)
            .await
            .unwrap()
            .into_iter()
            .find(|e| e.id == id)
            .unwrap()
    }

    #[tokio::test]
    async fn a_correction_keeps_who_published_and_when() {
        let pool = test_pool().await;
        let jane = member(&pool, "jane@example.com", "Jane").await;
        let paul = member(&pool, "paul@example.com", "Paul").await;
        let id = incident(&pool, jane).await;
        assert!(
            EventReviewRepository::find(&pool, id)
                .await
                .unwrap()
                .is_none()
        );

        let first = EventReviewRepository::save(&pool, id, "A disk filled up.", jane)
            .await
            .unwrap();
        sqlx::query("UPDATE event_reviews SET published_at = ?")
            .bind(LONG_AGO)
            .execute(&pool)
            .await
            .unwrap();
        let second = EventReviewRepository::save(&pool, id, "A log disk filled up.", paul)
            .await
            .unwrap();

        assert_eq!(
            (first, second),
            (ReviewSaved::Published, ReviewSaved::Corrected)
        );
        let review = EventReviewRepository::find(&pool, id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(review.body, "A log disk filled up.");
        assert_eq!(review.author_name, "Jane");
        assert_eq!(
            review.published_at.to_rfc3339(),
            "2026-10-01T09:00:00+00:00"
        );
    }

    #[tokio::test]
    async fn each_change_to_a_review_dates_its_feed_entry() {
        let pool = test_pool().await;
        let jane = member(&pool, "jane@example.com", "Jane").await;
        let id = incident(&pool, jane).await;
        let bare = incident(&pool, jane).await;
        let long_ago = "2026-10-01T09:00:00+00:00";

        for text in ["A disk filled up.", "A log disk filled up."] {
            date_events_long_ago(&pool).await;
            EventReviewRepository::save(&pool, id, text, jane)
                .await
                .unwrap();
            let entry = feed_entry(&pool, id).await;
            assert!(entry.updated_at.to_rfc3339().as_str() > long_ago, "{text}");
            assert_eq!(entry.review_body.as_deref(), Some(text));
            assert!(entry.has_review);
        }
        assert_eq!(feed_entry(&pool, bare).await.review_body, None);

        date_events_long_ago(&pool).await;
        EventReviewRepository::delete(&pool, id).await.unwrap();

        let entry = feed_entry(&pool, id).await;
        assert!(entry.updated_at.to_rfc3339().as_str() > long_ago);
        assert_eq!(entry.review_body, None);
        assert!(!entry.has_review);
        assert!(
            EventReviewRepository::find(&pool, id)
                .await
                .unwrap()
                .is_none()
        );
    }
}
