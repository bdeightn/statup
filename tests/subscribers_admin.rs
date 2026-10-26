//! Integration tests for what an administrator sees and does about email
//! subscribers: the card of the Alerts page, the list, and removing one.

mod common;

use chrono::Utc;
use reqwest::StatusCode;

use common::{MailSink, Options, TestApp};
use statup::clock;
use statup::i18n::I18n;
use statup::models::{Choices, CreateEventInput, Kind, Role, Severity};
use statup::services::{EventService, SubscriptionService};

const ADMIN_PASSWORD: &str = "admin_password_12";
const READER_PASSWORD: &str = "reader_password_12";
const ALERTS: &str = "/admin/notifications";
const LIST: &str = "/admin/notifications/subscribers";
const SWITCH: &str = "/admin/notifications/visitor-subscriptions";
const UNCHECKED_SWITCH: &str = r#"name="open" value="on" class="checkbox">"#;
const CHECKED_SWITCH: &str = r#"name="open" value="on" class="checkbox" checked>"#;
const PUBLISHED_AND_INCIDENTS: Choices = Choices {
    incidents: true,
    maintenances: false,
    publications: true,
};

impl TestApp {
    /// Signs in as an administrator and returns their id.
    async fn as_admin(&self) -> i64 {
        let id = self
            .create_user("admin@example.com", ADMIN_PASSWORD, "Admin", Role::Admin)
            .await;
        self.login("admin@example.com", ADMIN_PASSWORD).await;
        id
    }

    async fn as_reader(&self) {
        self.create_user(
            "reader@example.com",
            READER_PASSWORD,
            "Reader",
            Role::Reader,
        )
        .await;
        self.login("reader@example.com", READER_PASSWORD).await;
    }

    /// A page read in English, whatever the instance's default language.
    async fn english(&self, path: &str) -> (StatusCode, String) {
        let response = self
            .client
            .get(self.url(path))
            .header("accept-language", "en")
            .send()
            .await
            .expect("GET");
        let status = response.status();
        (status, response.text().await.unwrap_or_default())
    }

    async fn subscribe_visitor(&self, email: &str, choices: Choices) -> i64 {
        let token = match SubscriptionService::request(&self.pool, email, "en", choices).await {
            Ok(Some(token)) => token,
            _ => panic!("no link issued for {email}"),
        };
        SubscriptionService::confirm(&self.pool, &token)
            .await
            .expect("confirmed")
            .expect("a subscriber")
            .id
    }

    async fn subscribe_member(&self, name: &str, email: &str, choices: Choices) -> i64 {
        let user_id = self
            .create_user(email, READER_PASSWORD, name, Role::Reader)
            .await;
        SubscriptionService::save_member(&self.pool, user_id, "en", choices)
            .await
            .expect("member subscribed");
        sqlx::query_scalar("SELECT id FROM subscribers WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .expect("subscriber id")
    }

    async fn journal(&self, kind: &str) -> Vec<(Option<String>, Option<String>)> {
        sqlx::query_as("SELECT subject, detail FROM journal_entries WHERE kind = ? ORDER BY id")
            .bind(kind)
            .fetch_all(&self.pool)
            .await
            .expect("journal read")
    }

    async fn delivery_count(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM subscriber_deliveries")
            .fetch_one(&self.pool)
            .await
            .expect("deliveries counted")
    }

    async fn publish_incident(&self, author_id: i64) -> i64 {
        let input = CreateEventInput {
            kind: Kind::Incident,
            severity: Some(Severity::Minor),
            planned: false,
            category: None,
            title: "Slow pages".to_string(),
            description: "Pages take longer than usual.".to_string(),
            planned_start: None,
            planned_end: None,
            started_at: None,
            opening_step: None,
            keeps_services_up: false,
            service_ids: Vec::new(),
            follows_event_id: None,
            author_id,
            api_key_id: None,
            next_update: None,
        };
        EventService::create(&self.pool, input)
            .await
            .expect("incident created")
            .id
    }

    async fn fail_delivery(&self, subscriber_id: i64, event_id: i64) {
        sqlx::query(
            "INSERT INTO subscriber_deliveries (subscriber_id, happening, event_id, status, \
             failure, next_attempt_at) VALUES (?, 'opened', ?, 'failed', 'no_mail_server', \
             datetime('now'))",
        )
        .bind(subscriber_id)
        .bind(event_id)
        .execute(&self.pool)
        .await
        .expect("delivery inserted");
    }
}

/// An instance that can send email.
async fn with_mail() -> (TestApp, MailSink) {
    let sink = MailSink::start().await;
    let app = TestApp::spawn_with(Options {
        mail_port: Some(sink.port),
        ..Options::default()
    })
    .await;
    (app, sink)
}

#[tokio::test]
async fn a_reader_may_neither_see_the_subscribers_nor_change_them() {
    let (app, _sink) = with_mail().await;
    let ana = app
        .subscribe_member("Ana", "ana@example.com", Choices::EVERYTHING)
        .await;
    app.as_reader().await;

    let (status, _) = app.english(LIST).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let csrf = app.csrf_from("/profile").await;
    let remove = format!("{LIST}/{ana}/remove");
    let (status, _, _) = app.post_form(&remove, &csrf, &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = app.post_form(SWITCH, &csrf, &[("open", "on")]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    assert!(!SubscriptionService::visitors_open(&app.pool).await.unwrap());
    let still_there: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM subscribers")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(still_there, 1);
}

#[tokio::test]
async fn the_alerts_card_counts_the_subscribers_and_the_switch_starts_off() {
    let (app, _sink) = with_mail().await;
    app.as_admin().await;

    let (_, page) = app.english(ALERTS).await;
    assert!(page.contains("Email subscribers"));
    assert!(page.contains(r#"Email subscribers <span class="count">0</span>"#));
    assert!(!page.contains("See the subscribers"));
    assert!(page.contains(UNCHECKED_SWITCH), "off on a new instance");

    app.subscribe_member("Ana", "ana@example.com", Choices::EVERYTHING)
        .await;
    let (_, page) = app.english(ALERTS).await;
    assert!(page.contains(r#"Email subscribers <span class="count">1</span>"#));
    app.subscribe_visitor("ben@example.com", Choices::EVERYTHING)
        .await;
    let (_, page) = app.english(ALERTS).await;
    assert!(page.contains(r#"Email subscribers <span class="count">2</span>"#));
    assert!(page.contains(r#"href="/admin/notifications/subscribers""#));
}

#[tokio::test]
async fn without_a_mail_server_the_card_says_so_and_has_no_switch() {
    let app = TestApp::spawn().await;
    app.as_admin().await;

    let (_, page) = app.english(ALERTS).await;

    assert!(page.contains("Subscriptions need an SMTP server."));
    assert!(!page.contains(r#"name="open""#));
}

#[tokio::test]
async fn ticking_the_switch_opens_subscriptions_and_journals_only_a_change() {
    let (app, _sink) = with_mail().await;
    app.as_admin().await;
    let csrf = app.csrf_from(ALERTS).await;

    let (status, _, location) = app.post_form(SWITCH, &csrf, &[("open", "on")]).await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some("/admin/notifications?notice=visitors_open")
    );
    assert!(SubscriptionService::visitors_open(&app.pool).await.unwrap());
    let (_, page) = app
        .english("/admin/notifications?notice=visitors_open")
        .await;
    assert!(page.contains("Visitors can now subscribe."));
    assert!(page.contains(CHECKED_SWITCH));
    assert_eq!(
        app.journal("visitor_subscriptions_changed").await,
        [(None, Some("on".to_string()))]
    );

    app.post_form(SWITCH, &csrf, &[("open", "on")]).await;
    assert_eq!(app.journal("visitor_subscriptions_changed").await.len(), 1);

    let (_, _, location) = app.post_form(SWITCH, &csrf, &[]).await;
    assert_eq!(
        location.as_deref(),
        Some("/admin/notifications?notice=visitors_closed")
    );
    assert!(!SubscriptionService::visitors_open(&app.pool).await.unwrap());
    let (_, page) = app
        .english("/admin/notifications?notice=visitors_closed")
        .await;
    assert!(page.contains("Visitors can no longer subscribe"));
    let entries = app.journal("visitor_subscriptions_changed").await;
    assert_eq!(entries.last(), Some(&(None, Some("off".to_string()))));
    assert_eq!(entries.len(), 2);
}

#[tokio::test]
async fn the_list_shows_who_receives_what_since_when_and_how_the_last_email_went() {
    let (app, _sink) = with_mail().await;
    let admin = app.as_admin().await;
    let event_id = app.publish_incident(admin).await;
    app.subscribe_member("Ana", "ana@example.com", Choices::EVERYTHING)
        .await;
    let ben = app
        .subscribe_visitor("ben@example.com", PUBLISHED_AND_INCIDENTS)
        .await;
    app.fail_delivery(ben, event_id).await;

    let (status, page) = app.english(LIST).await;

    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Ana"));
    assert!(page.contains("Member"));
    assert!(page.contains("ana@example.com"));
    assert!(page.contains("incidents, maintenance, announcements"));
    assert!(page.contains("Nothing sent"));
    assert!(page.contains("ben@example.com"));
    assert!(page.contains("Visitor"));
    assert!(page.contains("incidents, announcements"));
    assert!(page.contains("SMTP server not configured"));
    let date = I18n::new("en").format_date_long(&clock::local_date(&Utc::now()));
    assert!(page.contains(&format!("since {date}")));
    assert!(
        page.contains("Visitor subscriptions are closed"),
        "a visitor is subscribed while subscriptions are closed"
    );
}

#[tokio::test]
async fn an_empty_list_says_so_and_a_members_only_page_pauses_its_visitors() {
    let (app, _sink) = with_mail().await;
    app.as_admin().await;

    let (_, page) = app.english(LIST).await;
    assert!(page.contains("No subscribers."));

    SubscriptionService::set_visitors_open(&app.pool, true)
        .await
        .unwrap();
    app.subscribe_visitor("ben@example.com", Choices::EVERYTHING)
        .await;
    let (_, page) = app.english(LIST).await;
    assert!(
        page.contains("Visitor subscriptions are closed"),
        "the page is members-only, so no visitor can read the news"
    );
}

#[tokio::test]
async fn removing_a_subscriber_drops_them_and_their_emails_without_naming_them_in_the_address() {
    let (app, _sink) = with_mail().await;
    let admin = app.as_admin().await;
    let event_id = app.publish_incident(admin).await;
    let ben = app
        .subscribe_visitor("ben@example.com", Choices::EVERYTHING)
        .await;
    let ana = app
        .subscribe_member("Ana", "ana@example.com", Choices::EVERYTHING)
        .await;
    app.fail_delivery(ben, event_id).await;
    let remove = format!("{LIST}/{ben}/remove");

    let without_token = app
        .client
        .post(app.url(&remove))
        .send()
        .await
        .expect("POST")
        .status();
    assert_eq!(without_token, StatusCode::FORBIDDEN);
    assert_eq!(app.delivery_count().await, 1, "refused, nothing removed");

    let csrf = app.csrf_from(LIST).await;
    let (status, _, location) = app.post_form(&remove, &csrf, &[]).await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some("/admin/notifications/subscribers?done=removed")
    );
    assert_eq!(app.delivery_count().await, 0);
    assert_eq!(
        app.journal("subscriber_removed").await,
        [(Some("ben@example.com".to_string()), None)]
    );
    let (_, page) = app
        .english("/admin/notifications/subscribers?done=removed")
        .await;
    assert!(page.contains("Subscriber removed"));
    assert!(!page.contains("ben@example.com"));
    assert!(page.contains("ana@example.com"));

    let (status, _, _) = app.post_form(&remove, &csrf, &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = app
        .post_form(&format!("{LIST}/{ana}/remove"), &csrf, &[])
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        app.journal("subscriber_removed").await.last(),
        Some(&(Some("ana@example.com".to_string()), None)),
        "a member is named by the address of their account"
    );
}
