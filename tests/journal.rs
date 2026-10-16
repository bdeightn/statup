//! Integration tests for the Journal page: who may read it, its lines and
//! filters, and what it captures and for how long.

mod common;

use reqwest::StatusCode;

use common::TestApp;
use statup::models::{JournalKind, Role};
use statup::repositories::{JournalRepository, NewJournalEntry};

const ADMIN_EMAIL: &str = "admin@test.com";
const ADMIN_PASSWORD: &str = "admin_password_12";

async fn spawn_with_admin() -> TestApp {
    let app = TestApp::spawn().await;
    app.create_user(ADMIN_EMAIL, ADMIN_PASSWORD, "Admin", Role::Admin)
        .await;
    app.login(ADMIN_EMAIL, ADMIN_PASSWORD).await;
    app
}

async fn save_capture(
    app: &TestApp,
    capture: &str,
    days: &str,
) -> (StatusCode, String, Option<String>) {
    let csrf = app.csrf_from("/admin/journal").await;
    app.post_form(
        "/admin/journal/settings",
        &csrf,
        &[("capture", capture), ("retention_days", days)],
    )
    .await
}

#[tokio::test]
async fn only_administrators_read_the_journal() {
    let app = TestApp::spawn().await;
    app.create_user(
        "editor@test.com",
        "editor_password_12",
        "Editor",
        Role::Publisher,
    )
    .await;
    app.login("editor@test.com", "editor_password_12").await;

    let (status, _) = app.get("/admin/journal").await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_lines_are_listed_and_narrowed_by_family() {
    let app = spawn_with_admin().await;
    let (status, body) = app.get("/admin/journal").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Rien à signaler pour l"));

    let mut line = NewJournalEntry::new(JournalKind::AppError);
    line.subject = Some("Database is locked".into());
    JournalRepository::add(&app.pool, &line).await.unwrap();

    let (_, body) = app.get("/admin/journal").await;
    assert!(body.contains("Erreur : Database is locked"));
    let (_, body) = app.get("/admin/journal?category=members").await;
    assert!(!body.contains("Database is locked"));
    assert!(body.contains("Aucune ligne ne correspond"));
}

#[tokio::test]
async fn the_capture_and_retention_are_saved_and_shown_on_the_tile() {
    let app = spawn_with_admin().await;

    let (status, _, location) = save_capture(&app, "detailed", "30").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location.as_deref(), Some("/admin/journal?saved=1"));

    let (_, body) = app.get("/admin/settings").await;
    assert!(body.contains("Capture détaillée, 30 jours"));
}

#[tokio::test]
async fn a_retention_out_of_range_is_refused_in_the_page() {
    let app = spawn_with_admin().await;

    let (status, body, _) = save_capture(&app, "detailed", "400").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("La conservation va de 1 à 365 jours."));
    assert!(body.contains("value=\"400\""));
    let (_, body) = app.get("/admin/settings").await;
    assert!(body.contains("Capture essentielle, 90 jours"));
}

/// What the journal holds, oldest first: kind, essential, subject, detail.
type Line = (String, bool, Option<String>, Option<String>);

async fn lines(app: &TestApp) -> Vec<Line> {
    sqlx::query_as("SELECT kind, essential, subject, detail FROM journal_entries ORDER BY id")
        .fetch_all(&app.pool)
        .await
        .unwrap()
}

fn line(kind: &str, essential: bool, subject: Option<&str>, detail: Option<&str>) -> Line {
    (
        kind.to_string(),
        essential,
        subject.map(str::to_string),
        detail.map(str::to_string),
    )
}

#[tokio::test]
async fn adding_a_service_is_kept_as_an_essential_line() {
    let app = spawn_with_admin().await;
    let csrf = app.csrf_from("/services/new").await;

    let (status, _, _) = app
        .post_form(
            "/services/new",
            &csrf,
            &[("name", " Billing "), ("description", "")],
        )
        .await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        lines(&app).await,
        [line("service_created", true, Some("Billing"), None)]
    );
    let (_, page) = app.get("/admin/journal").await;
    assert!(page.contains("Admin a ajouté le service «\u{a0}Billing\u{a0}»"));
}

#[tokio::test]
async fn adding_a_member_names_them_and_never_keeps_their_password() {
    let app = spawn_with_admin().await;
    let csrf = app.csrf_from("/admin/users").await;

    let (status, _, _) = app
        .post_form(
            "/admin/users/new",
            &csrf,
            &[
                ("display_name", "Paul"),
                ("email", "paul@test.com"),
                ("role", "publisher"),
            ],
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        lines(&app).await,
        [line("member_added", true, Some("Paul"), Some("publisher"))]
    );
    let kept: String = sqlx::query_scalar(
        "SELECT COALESCE(group_concat(COALESCE(subject, '') || COALESCE(detail, '')), '') \
         FROM journal_entries",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert!(!kept.contains("paul@test.com"));
}

#[tokio::test]
async fn a_channel_is_named_when_added_and_when_deleted() {
    let app = spawn_with_admin().await;
    let csrf = app.csrf_from("/admin/notifications").await;
    let fields = [
        ("name", "IT team"),
        ("kind", "webhook"),
        ("target", "https://example.com/hook/SECRET"),
        ("locale", "en"),
        ("on_incidents", "on"),
    ];
    let (_, _, location) = app.post_form("/admin/notifications", &csrf, &fields).await;
    let id = location
        .and_then(|l| l.rsplit('=').next().map(str::to_string))
        .unwrap();

    let (status, _, _) = app
        .post_form(&format!("/admin/notifications/{id}/delete"), &csrf, &[])
        .await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        lines(&app).await,
        [
            line("channel_created", true, Some("IT team"), None),
            line("channel_deleted", true, Some("IT team"), None),
        ]
    );
}

#[tokio::test]
async fn publishing_an_event_waits_for_the_detailed_capture() {
    let app = spawn_with_admin().await;
    let fields = [
        ("title", "Database outage"),
        ("description", "The primary database is down"),
        ("kind", "incident"),
        ("severity", "critical"),
    ];
    let csrf = app.csrf_from("/events/new").await;
    let (status, _, _) = app.post_form("/events/new", &csrf, &fields).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(lines(&app).await.is_empty());

    save_capture(&app, "detailed", "30").await;
    let csrf = app.csrf_from("/events/new").await;
    app.post_form("/events/new", &csrf, &fields).await;

    assert_eq!(
        lines(&app).await,
        [
            line(
                "journal_settings_saved",
                true,
                None,
                Some("capture=detailed, retention=30 days")
            ),
            line("event_created", false, Some("Database outage"), None),
        ]
    );
}

#[tokio::test]
async fn deleting_an_event_is_essential_and_keeps_its_title() {
    let app = spawn_with_admin().await;
    let csrf = app.csrf_from("/events/new").await;
    let (_, _, location) = app
        .post_form(
            "/events/new",
            &csrf,
            &[
                ("title", "Wrong alarm"),
                ("description", "Nothing happened"),
                ("kind", "incident"),
                ("severity", "minor"),
            ],
        )
        .await;
    let path = location.unwrap();
    let id = path
        .trim_start_matches("/events/")
        .split('?')
        .next()
        .unwrap();

    let (status, _, _) = app
        .post_form(&format!("/events/{id}/delete"), &csrf, &[])
        .await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        lines(&app).await,
        [line("event_deleted", true, Some("Wrong alarm"), None)]
    );
}

#[tokio::test]
async fn saving_the_general_settings_names_what_changed() {
    let app = spawn_with_admin().await;

    let (status, _, _) = app.save_settings(&[("instance_name", "Acme")], None).await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        lines(&app).await,
        [line("settings_saved", true, None, Some("name"))]
    );
}
