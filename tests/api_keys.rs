//! Integration tests for the API keys page: who reaches it, a key shown
//! once and kept as a hash, a refused name, a revoked key.

mod common;

use reqwest::StatusCode;

use common::TestApp;
use statup::models::Role;
use statup::services::ApiKeyService;

const ADMIN_EMAIL: &str = "admin@test.com";
const ADMIN_PASSWORD: &str = "admin_password_12";

async fn spawn_with_admin() -> TestApp {
    let app = TestApp::spawn().await;
    app.create_user(ADMIN_EMAIL, ADMIN_PASSWORD, "Admin", Role::Admin)
        .await;
    app.login(ADMIN_EMAIL, ADMIN_PASSWORD).await;
    app
}

/// The key a creation page holds, read from its masked field.
fn key_in(page: &str) -> String {
    let start = page.find(r#"id="api-key""#).expect("the key is shown");
    let field = &page[start..page[start..].find('>').expect("field") + start];
    assert!(field.contains(r#"type="password""#), "the key is masked");
    let value = field.find(r#"value=""#).expect("the key's value") + 7;
    let end = field[value..].find('"').expect("end of the key");
    field[value..value + end].to_string()
}

async fn journal_lines(app: &TestApp) -> Vec<(String, String)> {
    sqlx::query_as("SELECT kind, subject FROM journal_entries ORDER BY id")
        .fetch_all(&app.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn only_an_admin_reaches_the_keys() {
    let app = TestApp::spawn().await;
    app.create_user(
        "editor@test.com",
        "editor_password_12",
        "Ed",
        Role::Publisher,
    )
    .await;
    app.login("editor@test.com", "editor_password_12").await;

    let (status, _) = app.get("/admin/api-keys").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_key_is_shown_once_in_the_answer_and_works_from_then_on() {
    let app = spawn_with_admin().await;
    let (status, body) = app.get("/admin/api-keys").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Aucune clé."));

    let csrf = app.csrf_from("/admin/api-keys").await;
    let resp = app
        .client
        .post(app.url("/admin/api-keys"))
        .form(&[("csrf_token", csrf.as_str()), ("name", "  Zabbix ")])
        .send()
        .await
        .expect("POST failed");
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store"),
        "a page showing a key is not stored"
    );
    let body = resp.text().await.unwrap_or_default();
    assert!(body.contains(r#"data-replace-url="/admin/api-keys""#));
    assert!(body.contains("<strong>Zabbix</strong>"));
    let key = key_in(&body);
    assert!(key.starts_with("stp_"));

    let holder = ApiKeyService::holder_of(&app.pool, &key)
        .await
        .unwrap()
        .expect("the key shown is the key kept");
    assert_eq!(holder.name, "Zabbix");

    let (_, body) = app.get("/admin/api-keys").await;
    assert!(!body.contains(&key), "and never again");
    assert!(body.contains(&key[..8]), "only its start is listed");
    assert!(body.contains("Dernière utilisation"));
    assert_eq!(
        journal_lines(&app).await,
        [("api_key_created".to_string(), "Zabbix".to_string())]
    );

    let (_, settings) = app.get("/admin/settings").await;
    assert!(settings.contains(r#"<span class="manage-tile-meta">1 clé</span>"#));
}

#[tokio::test]
async fn a_name_in_use_is_refused_with_the_form_open() {
    let app = spawn_with_admin().await;
    let csrf = app.csrf_from("/admin/api-keys").await;
    app.post_form("/admin/api-keys", &csrf, &[("name", "Zabbix")])
        .await;

    let (status, body, _) = app
        .post_form("/admin/api-keys", &csrf, &[("name", "ZABBIX")])
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Une clé en service porte déjà ce nom."));
    assert!(body.contains(r#"class="card-add key-add is-open""#));
    assert!(body.contains(r#"value="ZABBIX""#));
    assert!(!body.contains(r#"id="api-key""#));
}

#[tokio::test]
async fn a_revoked_key_stops_working_and_leaves_the_list() {
    let app = spawn_with_admin().await;
    let csrf = app.csrf_from("/admin/api-keys").await;
    let (_, body, _) = app
        .post_form("/admin/api-keys", &csrf, &[("name", "Zabbix")])
        .await;
    let key = key_in(&body);
    let holder = ApiKeyService::holder_of(&app.pool, &key)
        .await
        .unwrap()
        .unwrap();

    let (status, _, location) = app
        .post_form(&format!("/admin/api-keys/{}/revoke", holder.id), &csrf, &[])
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location.as_deref(), Some("/admin/api-keys?revoked=1"));
    assert!(
        ApiKeyService::holder_of(&app.pool, &key)
            .await
            .unwrap()
            .is_none()
    );

    let (_, body) = app.get("/admin/api-keys?revoked=1").await;
    assert!(body.contains("Clé retirée."));
    assert!(body.contains("Aucune clé."));
    assert_eq!(
        journal_lines(&app).await,
        [
            ("api_key_created".to_string(), "Zabbix".to_string()),
            ("api_key_revoked".to_string(), "Zabbix".to_string()),
        ]
    );

    let (status, _, _) = app
        .post_form(&format!("/admin/api-keys/{}/revoke", holder.id), &csrf, &[])
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a key is revoked once");
}

#[tokio::test]
async fn a_regenerated_key_keeps_its_name_and_the_old_secret_stops_at_once() {
    let app = spawn_with_admin().await;
    let csrf = app.csrf_from("/admin/api-keys").await;
    let (_, body, _) = app
        .post_form("/admin/api-keys", &csrf, &[("name", "Zabbix")])
        .await;
    let old = key_in(&body);
    let holder = ApiKeyService::holder_of(&app.pool, &old)
        .await
        .unwrap()
        .unwrap();

    let resp = app
        .client
        .post(app.url(&format!("/admin/api-keys/{}/regenerate", holder.id)))
        .form(&[("csrf_token", csrf.as_str())])
        .send()
        .await
        .expect("POST failed");
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let body = resp.text().await.unwrap_or_default();
    assert!(body.contains("Nouvelle clé pour <strong>Zabbix</strong>"));
    let new = key_in(&body);
    assert_ne!(new, old);

    assert!(
        ApiKeyService::holder_of(&app.pool, &old)
            .await
            .unwrap()
            .is_none()
    );
    let renewed = ApiKeyService::holder_of(&app.pool, &new)
        .await
        .unwrap()
        .expect("the new secret works");
    assert_eq!((renewed.id, renewed.name.as_str()), (holder.id, "Zabbix"));
    assert_eq!(
        journal_lines(&app).await,
        [
            ("api_key_created".to_string(), "Zabbix".to_string()),
            ("api_key_regenerated".to_string(), "Zabbix".to_string()),
        ]
    );

    let (status, _, _) = app
        .post_form("/admin/api-keys/999/regenerate", &csrf, &[])
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
