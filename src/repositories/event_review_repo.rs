//! Reviews of resolved incidents: one per incident, published, corrected
//! and removed.

use sqlx::{QueryBuilder, Sqlite};

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
            sqlx::query(
                "UPDATE event_reviews SET body = ?, updated_at = datetime('now') \
                 WHERE event_id = ?",
            )
            .bind(body)
            .bind(event_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(if inserted == 0 {
            ReviewSaved::Corrected
        } else {
            ReviewSaved::Published
        })
    }

    /// The review texts of several events in one query, as `(event_id, body)`.
    pub async fn bodies_for(
        pool: &DbPool,
        event_ids: &[i64],
    ) -> Result<Vec<(i64, String)>, sqlx::Error> {
        if event_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut qb = QueryBuilder::<Sqlite>::new(
            "SELECT event_id, body FROM event_reviews WHERE event_id IN (",
        );
        let mut ids = qb.separated(", ");
        for id in event_ids {
            ids.push_bind(*id);
        }
        qb.push(")");
        qb.build_query_as::<(i64, String)>().fetch_all(pool).await
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

        let first = EventReviewRepository::save(&pool, id, "A disk filled up.", jane)
            .await
            .unwrap();
        sqlx::query("UPDATE event_reviews SET published_at = '2026-10-01 09:00:00'")
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
    async fn a_corrected_review_is_the_feed_activity_and_its_text_is_found() {
        let pool = test_pool().await;
        let jane = member(&pool, "jane@example.com", "Jane").await;
        let reviewed = incident(&pool, jane).await;
        let bare = incident(&pool, jane).await;
        EventReviewRepository::save(&pool, reviewed, "A disk filled up.", jane)
            .await
            .unwrap();
        sqlx::query("UPDATE events SET updated_at = '2026-10-01 09:00:00'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE event_reviews SET updated_at = '2026-10-02 09:00:00'")
            .execute(&pool)
            .await
            .unwrap();

        let feed = EventRepository::list_for_feed(&pool, 10).await.unwrap();
        let activity = feed
            .iter()
            .find(|e| e.id == reviewed)
            .unwrap()
            .last_activity_at;
        assert_eq!(
            activity.map(|at| at.to_rfc3339()),
            Some("2026-10-02T09:00:00+00:00".to_string())
        );
        let bodies = EventReviewRepository::bodies_for(&pool, &[reviewed, bare])
            .await
            .unwrap();
        assert_eq!(bodies, vec![(reviewed, "A disk filled up.".to_string())]);
        assert_eq!(
            EventReviewRepository::bodies_for(&pool, &[]).await.unwrap(),
            Vec::<(i64, String)>::new()
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
