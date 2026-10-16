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
