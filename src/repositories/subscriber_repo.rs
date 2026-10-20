//! The members and visitors told by email about what the page publishes.

use crate::db::DbPool;
use crate::models::{Choices, Subscriber, SubscriptionRequest};

const SUBSCRIBER_COLUMNS: &str = "id, user_id, email, locale, on_incidents, on_maintenances, \
     on_publications, unsubscribe_secret, confirmed_at, created_at";

pub struct SubscriberRepository;

impl SubscriberRepository {
    pub async fn find(pool: &DbPool, id: i64) -> Result<Option<Subscriber>, sqlx::Error> {
        sqlx::query_as::<_, Subscriber>(&format!(
            "SELECT {SUBSCRIBER_COLUMNS} FROM subscribers WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_for_member(
        pool: &DbPool,
        user_id: i64,
    ) -> Result<Option<Subscriber>, sqlx::Error> {
        sqlx::query_as::<_, Subscriber>(&format!(
            "SELECT {SUBSCRIBER_COLUMNS} FROM subscribers WHERE user_id = ?"
        ))
        .bind(user_id)
        .fetch_optional(pool)
        .await
    }

    /// Subscribes the visitor of a confirmed request, or gives the address
    /// already subscribed the choices of its newest request, and uses up
    /// every request of the address, all or nothing. A new subscriber gets
    /// `unsubscribe_secret`; one already there keeps theirs, so the links of
    /// the emails they received still work. Returns `None`, changing
    /// nothing, when the address has no request left: of two clicks at the
    /// same moment, only the first counts.
    pub async fn confirm_visitor(
        pool: &DbPool,
        request: &SubscriptionRequest,
        unsubscribe_secret: &str,
    ) -> Result<Option<i64>, sqlx::Error> {
        let mut tx = pool.begin().await?;
        let used = sqlx::query("DELETE FROM subscription_requests WHERE email = ?")
            .bind(&request.email)
            .execute(&mut *tx)
            .await?;
        if used.rows_affected() == 0 {
            return Ok(None);
        }
        let id = sqlx::query_scalar(
            "INSERT INTO subscribers (email, locale, on_incidents, on_maintenances, \
             on_publications, unsubscribe_secret) VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(email) DO UPDATE SET locale = excluded.locale, \
               on_incidents = excluded.on_incidents, \
               on_maintenances = excluded.on_maintenances, \
               on_publications = excluded.on_publications, \
               confirmed_at = datetime('now') \
             RETURNING id",
        )
        .bind(&request.email)
        .bind(&request.locale)
        .bind(request.choices.incidents)
        .bind(request.choices.maintenances)
        .bind(request.choices.publications)
        .bind(unsubscribe_secret)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(id))
    }

    /// Subscribes a member at the address of their account, or replaces
    /// their choices. Returns true when the subscription is new.
    pub async fn save_member(
        pool: &DbPool,
        user_id: i64,
        locale: &str,
        choices: Choices,
        unsubscribe_secret: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = pool.begin().await?;
        let created = sqlx::query(
            "INSERT INTO subscribers (user_id, locale, on_incidents, on_maintenances, \
             on_publications, unsubscribe_secret) VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(locale)
        .bind(choices.incidents)
        .bind(choices.maintenances)
        .bind(choices.publications)
        .bind(unsubscribe_secret)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            > 0;
        if !created {
            sqlx::query(
                "UPDATE subscribers SET locale = ?, on_incidents = ?, on_maintenances = ?, \
                 on_publications = ?, confirmed_at = datetime('now') WHERE user_id = ?",
            )
            .bind(locale)
            .bind(choices.incidents)
            .bind(choices.maintenances)
            .bind(choices.publications)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(created)
    }

    /// Ends a subscription, and with it the emails it still had to receive.
    pub async fn delete(pool: &DbPool, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM subscribers WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CreateEventInput, Kind, Role, Severity};
    use crate::repositories::{
        EventRepository, NewSubscriptionRequest, RequestLimits, SubscriptionRequestRepository,
        UserRepository,
    };
    use crate::test_helpers::test_pool;

    const EVERYTHING: Choices = Choices {
        incidents: true,
        maintenances: true,
        publications: true,
    };
    const ONLY_INCIDENTS: Choices = Choices {
        incidents: true,
        maintenances: false,
        publications: false,
    };

    async fn request(pool: &DbPool, email: &str, choices: Choices) -> SubscriptionRequest {
        let new = NewSubscriptionRequest {
            email,
            secret_hash: "hash",
            locale: "fr",
            choices,
        };
        let limits = RequestLimits {
            per_address: 100,
            per_instance: 100,
        };
        let id = SubscriptionRequestRepository::insert_within_limits(pool, &new, limits)
            .await
            .unwrap()
            .unwrap();
        SubscriptionRequestRepository::find_live(pool, id)
            .await
            .unwrap()
            .unwrap()
    }

    async fn confirmed(pool: &DbPool, email: &str, choices: Choices, secret: &str) -> i64 {
        let request = request(pool, email, choices).await;
        SubscriberRepository::confirm_visitor(pool, &request, secret)
            .await
            .unwrap()
            .unwrap()
    }

    async fn member(pool: &DbPool, email: &str) -> i64 {
        UserRepository::create(pool, email, "hash", "Someone", Role::Reader)
            .await
            .unwrap()
            .id
    }

    async fn count(pool: &DbPool, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn incident(pool: &DbPool) -> i64 {
        let author_id = member(pool, "ops@example.com").await;
        let input = CreateEventInput {
            kind: Kind::Incident,
            severity: Some(Severity::Minor),
            planned: false,
            category: None,
            title: "Network down".to_string(),
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

    async fn queue_for(pool: &DbPool, subscriber_id: i64, event_id: i64, happening: &str) -> bool {
        sqlx::query(
            "INSERT INTO subscriber_deliveries (subscriber_id, happening, event_id) \
             VALUES (?, ?, ?)",
        )
        .bind(subscriber_id)
        .bind(happening)
        .bind(event_id)
        .execute(pool)
        .await
        .is_ok()
    }

    #[tokio::test]
    async fn confirming_subscribes_the_visitor_with_their_choices() {
        let pool = test_pool().await;

        let id = confirmed(&pool, "ana@example.com", ONLY_INCIDENTS, "secret").await;

        let subscriber = SubscriberRepository::find(&pool, id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(subscriber.email.as_deref(), Some("ana@example.com"));
        assert_eq!(subscriber.user_id, None);
        assert_eq!(subscriber.locale, "fr");
        assert_eq!(subscriber.choices, ONLY_INCIDENTS);
        assert_eq!(subscriber.unsubscribe_secret, "secret");
        assert_eq!(count(&pool, "subscription_requests").await, 0);
    }

    #[tokio::test]
    async fn a_second_click_on_a_used_link_changes_nothing() {
        let pool = test_pool().await;
        let request = request(&pool, "ana@example.com", EVERYTHING).await;
        SubscriberRepository::confirm_visitor(&pool, &request, "first")
            .await
            .unwrap();

        let again = SubscriberRepository::confirm_visitor(&pool, &request, "second")
            .await
            .unwrap();

        assert_eq!(again, None);
        assert_eq!(count(&pool, "subscribers").await, 1);
    }

    #[tokio::test]
    async fn subscribing_again_replaces_the_choices_and_keeps_the_secret() {
        let pool = test_pool().await;
        let first = confirmed(&pool, "ana@example.com", EVERYTHING, "kept").await;

        let second = confirmed(&pool, "ANA@example.com", ONLY_INCIDENTS, "new").await;

        assert_eq!(first, second, "one address, one subscriber");
        let subscriber = SubscriberRepository::find(&pool, first)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(subscriber.choices, ONLY_INCIDENTS);
        assert_eq!(subscriber.unsubscribe_secret, "kept");
    }

    #[tokio::test]
    async fn a_member_subscribes_once_and_changes_their_choices() {
        let pool = test_pool().await;
        let ana = member(&pool, "ana@example.com").await;

        let created = SubscriberRepository::save_member(&pool, ana, "fr", EVERYTHING, "s1")
            .await
            .unwrap();
        let again = SubscriberRepository::save_member(&pool, ana, "en", ONLY_INCIDENTS, "s2")
            .await
            .unwrap();

        assert!(created);
        assert!(!again);
        let subscriber = SubscriberRepository::find_for_member(&pool, ana)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(subscriber.email, None);
        assert_eq!(subscriber.locale, "en");
        assert_eq!(subscriber.choices, ONLY_INCIDENTS);
        assert_eq!(subscriber.unsubscribe_secret, "s1");
    }

    #[tokio::test]
    async fn a_subscriber_is_a_member_or_an_address_never_both_nor_neither() {
        let pool = test_pool().await;
        let ana = member(&pool, "ana@example.com").await;
        let insert = |user_id: Option<i64>, email: Option<&'static str>| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO subscribers (user_id, email, locale, unsubscribe_secret) \
                     VALUES (?, ?, 'fr', 's')",
                )
                .bind(user_id)
                .bind(email)
                .execute(&pool)
                .await
                .is_ok()
            }
        };

        assert!(!insert(Some(ana), Some("ana@example.com")).await);
        assert!(!insert(None, None).await);
        assert!(insert(Some(ana), None).await);
        assert!(insert(None, Some("ana@example.com")).await);
    }

    #[tokio::test]
    async fn ending_a_subscription_drops_its_waiting_emails() {
        let pool = test_pool().await;
        let id = confirmed(&pool, "ana@example.com", EVERYTHING, "s").await;
        let event_id = incident(&pool).await;
        assert!(queue_for(&pool, id, event_id, "opened").await);

        assert!(SubscriberRepository::delete(&pool, id).await.unwrap());

        assert!(!SubscriberRepository::delete(&pool, id).await.unwrap());
        assert_eq!(count(&pool, "subscriber_deliveries").await, 0);
    }

    #[tokio::test]
    async fn deleting_a_member_or_an_event_takes_what_hangs_on_them() {
        let pool = test_pool().await;
        let ana = member(&pool, "ana@example.com").await;
        SubscriberRepository::save_member(&pool, ana, "fr", EVERYTHING, "s")
            .await
            .unwrap();
        let visitor = confirmed(&pool, "ben@example.com", EVERYTHING, "s").await;
        let event_id = incident(&pool).await;
        assert!(queue_for(&pool, visitor, event_id, "opened").await);

        sqlx::query("DELETE FROM events WHERE id = ?")
            .bind(event_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(ana)
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(count(&pool, "subscriber_deliveries").await, 0);
        assert_eq!(count(&pool, "subscribers").await, 1, "the visitor stays");
    }

    #[tokio::test]
    async fn only_published_news_can_be_queued_for_a_subscriber() {
        let pool = test_pool().await;
        let id = confirmed(&pool, "ana@example.com", EVERYTHING, "s").await;
        let event_id = incident(&pool).await;

        for happening in ["service_down", "service_up", "test"] {
            assert!(
                !queue_for(&pool, id, event_id, happening).await,
                "{happening}"
            );
        }
        assert!(queue_for(&pool, id, event_id, "reviewed").await);
    }
}
