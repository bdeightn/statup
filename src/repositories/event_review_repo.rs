//! Reviews of resolved incidents: one per incident, published, corrected
//! and removed.

use crate::db::DbPool;
use crate::models::EventReview;

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
    /// correction keeps who published it and when.
    pub async fn save(
        pool: &DbPool,
        event_id: i64,
        body: &str,
        author_id: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO event_reviews (event_id, body, author_id) VALUES (?, ?, ?) \
             ON CONFLICT (event_id) DO UPDATE SET body = excluded.body, \
             updated_at = datetime('now')",
        )
        .bind(event_id)
        .bind(body)
        .bind(author_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete(pool: &DbPool, event_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM event_reviews WHERE event_id = ?")
            .bind(event_id)
            .execute(pool)
            .await?;
        Ok(())
    }
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

        EventReviewRepository::save(&pool, id, "A disk filled up.", jane)
            .await
            .unwrap();
        sqlx::query("UPDATE event_reviews SET published_at = '2026-10-01 09:00:00'")
            .execute(&pool)
            .await
            .unwrap();
        EventReviewRepository::save(&pool, id, "A log disk filled up.", paul)
            .await
            .unwrap();

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
    async fn a_removed_review_is_gone_and_the_summary_says_so() {
        let pool = test_pool().await;
        let jane = member(&pool, "jane@example.com", "Jane").await;
        let id = incident(&pool, jane).await;
        let has_review = |pool: DbPool| async move {
            EventRepository::list_for_feed(&pool, 10).await.unwrap()[0].has_review
        };

        EventReviewRepository::save(&pool, id, "A disk filled up.", jane)
            .await
            .unwrap();
        assert!(has_review(pool.clone()).await);

        EventReviewRepository::delete(&pool, id).await.unwrap();
        assert!(
            EventReviewRepository::find(&pool, id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(!has_review(pool.clone()).await);
    }
}
