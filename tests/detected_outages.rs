//! A service's side panel lists its detected outages, cut to what each
//! viewer may read.

mod common;

use chrono::{Duration, Utc};

use common::TestApp;
use statup::models::Role;
use statup::repositories::{OutageCause, ServiceRepository};

const PASSWORD: &str = "a_long_pass_1234";

async fn service_with_an_outage(app: &TestApp, name: &str, cause: OutageCause) -> i64 {
    let service =
        ServiceRepository::create(&app.pool, name, &name.to_lowercase(), None, None, None)
            .await
            .expect("failed to create service");
    let start = Utc::now() - Duration::hours(3);
    ServiceRepository::mark_detected_down(&app.pool, service.id, start, &cause)
        .await
        .expect("failed to open the outage");
    ServiceRepository::mark_detected_up(&app.pool, service.id, start + Duration::minutes(46))
        .await
        .expect("failed to close the outage");
    service.id
}

fn checks_found(words: &str) -> OutageCause {
    OutageCause::Check(Some(words.to_string()))
}

#[tokio::test]
async fn visitors_read_when_and_how_long_but_not_why() {
    let app = TestApp::spawn_public().await;
    let id = service_with_an_outage(&app, "Mail", checks_found("name not found")).await;

    let (_, panel) = app.get(&format!("/services/{id}/drawer")).await;

    assert!(panel.contains("Pannes détectées"));
    assert!(panel.contains("46\u{a0}min"));
    assert!(!panel.contains("Nom introuvable"));
    assert!(!panel.contains("/admin/journal"));
}

#[tokio::test]
async fn members_read_why_but_not_the_journal() {
    let app = TestApp::spawn().await;
    let id = service_with_an_outage(&app, "Mail", checks_found("name not found")).await;
    let tool = service_with_an_outage(&app, "Wiki", OutageCause::Report).await;
    app.create_user("reader@example.com", PASSWORD, "Reader", Role::Reader)
        .await;
    app.login("reader@example.com", PASSWORD).await;

    let (_, panel) = app.get(&format!("/services/{id}/drawer")).await;
    assert!(panel.contains("Nom introuvable"));
    assert!(!panel.contains("/admin/journal"));
    let (_, panel) = app.get(&format!("/services/{tool}/drawer")).await;
    assert!(panel.contains("Signalée par un outil de supervision"));
}

#[tokio::test]
async fn administrators_get_the_journal_of_the_outage() {
    let app = TestApp::spawn().await;
    let id = service_with_an_outage(&app, "Mail", checks_found("name not found")).await;
    app.create_user("admin@example.com", PASSWORD, "Admin", Role::Admin)
        .await;
    app.login("admin@example.com", PASSWORD).await;

    let (_, panel) = app.get(&format!("/services/{id}/drawer")).await;

    assert!(panel.contains("/admin/journal?category=monitoring&#38;from="));
    assert!(panel.contains("q=Mail"));
}

#[tokio::test]
async fn a_short_outage_colours_nothing_and_is_not_listed() {
    let app = TestApp::spawn_public().await;
    let service = ServiceRepository::create(&app.pool, "Mail", "mail", None, None, None)
        .await
        .unwrap();
    let start = Utc::now() - Duration::hours(1);
    ServiceRepository::mark_detected_down(
        &app.pool,
        service.id,
        start,
        &checks_found("unreachable"),
    )
    .await
    .unwrap();
    ServiceRepository::mark_detected_up(&app.pool, service.id, start + Duration::minutes(5))
        .await
        .unwrap();

    let (_, panel) = app.get(&format!("/services/{}/drawer", service.id)).await;

    assert!(!panel.contains("Pannes détectées"));
}

#[tokio::test]
async fn the_panel_opens_folded_or_on_the_day_clicked() {
    let app = TestApp::spawn_public().await;
    let id = service_with_an_outage(&app, "Mail", checks_found("name not found")).await;
    sqlx::query("UPDATE services SET created_at = datetime('now', '-60 days')")
        .execute(&app.pool)
        .await
        .unwrap();
    let day: String =
        sqlx::query_scalar("SELECT date(started_at) FROM service_outages WHERE service_id = ?")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .unwrap();

    let (_, folded) = app.get(&format!("/services/{id}/drawer")).await;
    assert!(folded.contains(
        r#"id="svc-outages" class="drawer-section" aria-labelledby="svc-outages-title" hidden"#
    ));
    assert!(folded.contains(&format!(
        r#"data-outage-toggle="{day}" aria-controls="svc-outages" aria-pressed="false""#
    )));

    let (_, opened) = app.get(&format!("/services/{id}/drawer?day={day}")).await;
    assert!(opened.contains(r#"aria-labelledby="svc-outages-title">"#));
    assert!(opened.contains(&format!(
        r#"data-outage-toggle="{day}" aria-controls="svc-outages" aria-pressed="true""#
    )));
    assert!(opened.contains(&format!(r#"data-outage-days="{day}">"#)));

    let (_, home) = app.get("/").await;
    assert!(
        home.contains(&format!(r#"data-outage-day="{day}""#)),
        "the row's day opens the panel on it"
    );
}

async fn outages_of(app: &TestApp, service_id: i64) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM service_outages WHERE service_id = ?")
        .bind(service_id)
        .fetch_one(&app.pool)
        .await
        .unwrap()
}

async fn outage_id(app: &TestApp, service_id: i64) -> i64 {
    sqlx::query_scalar("SELECT id FROM service_outages WHERE service_id = ?")
        .bind(service_id)
        .fetch_one(&app.pool)
        .await
        .unwrap()
}

async fn signed_in(app: &TestApp, role: Role) {
    app.create_user("member@example.com", PASSWORD, "Member", role)
        .await;
    app.login("member@example.com", PASSWORD).await;
}

#[tokio::test]
async fn an_editor_sees_the_button_and_how_many_services_fell_together() {
    let app = TestApp::spawn().await;
    let mail = service_with_an_outage(&app, "Mail", checks_found("unreachable")).await;
    service_with_an_outage(&app, "Wiki", checks_found("unreachable")).await;
    service_with_an_outage(&app, "VPN", checks_found("unreachable")).await;
    signed_in(&app, Role::Publisher).await;

    let (_, panel) = app.get(&format!("/services/{mail}/drawer")).await;

    assert!(panel.contains("était pas une panne"));
    assert!(panel.contains("Effacer aussi chez les 2 autres services tombés au même moment"));
}

#[tokio::test]
async fn a_reader_neither_sees_nor_may_erase() {
    let app = TestApp::spawn().await;
    let mail = service_with_an_outage(&app, "Mail", checks_found("unreachable")).await;
    let outage = outage_id(&app, mail).await;
    signed_in(&app, Role::Reader).await;

    let (_, panel) = app.get(&format!("/services/{mail}/drawer")).await;
    assert!(!panel.contains("était pas une panne"));
    let csrf = app.csrf_from("/").await;
    let (status, _, _) = app
        .post_form(
            &format!("/services/{mail}/outages/{outage}/erase"),
            &csrf,
            &[],
        )
        .await;

    assert!(status.is_client_error(), "{status}");
    assert_eq!(outages_of(&app, mail).await, 1);
}

#[tokio::test]
async fn an_editor_erases_one_outage_or_those_that_fell_with_it() {
    let app = TestApp::spawn().await;
    let mail = service_with_an_outage(&app, "Mail", checks_found("unreachable")).await;
    let wiki = service_with_an_outage(&app, "Wiki", checks_found("unreachable")).await;
    let vpn = service_with_an_outage(&app, "VPN", checks_found("unreachable")).await;
    signed_in(&app, Role::Publisher).await;
    let csrf = app.csrf_from("/").await;

    let alone = outage_id(&app, mail).await;
    let (status, _, _) = app
        .post_form(
            &format!("/services/{mail}/outages/{alone}/erase"),
            &csrf,
            &[],
        )
        .await;
    assert!(status.is_redirection(), "{status}");
    assert_eq!(
        (outages_of(&app, mail).await, outages_of(&app, wiki).await),
        (0, 1)
    );

    let together = outage_id(&app, wiki).await;
    app.post_form(
        &format!("/services/{wiki}/outages/{together}/erase"),
        &csrf,
        &[("together", "on")],
    )
    .await;
    assert_eq!(outages_of(&app, vpn).await, 0);

    let lines: Vec<(String, String)> = sqlx::query_as(
        "SELECT subject, detail FROM journal_entries WHERE kind = 'outage_erased' ORDER BY id",
    )
    .fetch_all(&app.pool)
    .await
    .unwrap();
    let subjects: Vec<&str> = lines.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(subjects, ["Mail", "Wiki", "VPN"]);
    assert!(lines[2].1.ends_with(" · Wiki"), "{}", lines[2].1);
}

#[tokio::test]
async fn an_outage_under_way_is_not_erased() {
    let app = TestApp::spawn().await;
    let service = ServiceRepository::create(&app.pool, "Mail", "mail", None, None, None)
        .await
        .unwrap();
    ServiceRepository::mark_detected_down(
        &app.pool,
        service.id,
        Utc::now() - Duration::hours(1),
        &checks_found("unreachable"),
    )
    .await
    .unwrap();
    let outage = outage_id(&app, service.id).await;
    signed_in(&app, Role::Publisher).await;
    let csrf = app.csrf_from("/").await;

    let (status, _, _) = app
        .post_form(
            &format!("/services/{}/outages/{outage}/erase", service.id),
            &csrf,
            &[],
        )
        .await;

    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
    assert_eq!(outages_of(&app, service.id).await, 1);
}

#[tokio::test]
async fn a_day_that_is_not_a_date_opens_the_panel_folded() {
    let app = TestApp::spawn_public().await;
    let id = service_with_an_outage(&app, "Mail", checks_found("name not found")).await;

    let (status, panel) = app
        .get(&format!("/services/{id}/drawer?day=yesterday"))
        .await;

    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(panel.contains(r#"aria-labelledby="svc-outages-title" hidden"#));
}
