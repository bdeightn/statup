//! Importing the monitors of an Uptime Kuma status page, from "New" on the
//! Services page: read the page, tick its groups, import.

mod common;

use reqwest::StatusCode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use common::TestApp;
use statup::i18n::{I18n, LANGUAGES};
use statup::models::Role;
use statup::repositories::{FolderRepository, ServiceRepository};

const PUBLISHER_EMAIL: &str = "editor@test.com";
const PUBLISHER_PASSWORD: &str = "editor_password_12";

/// The data of a page named `infra`, as Kuma 2.x sends it.
const PAGE: &str = r#"{"config": {"slug": "infra", "title": "Infra"}, "incidents": [],
    "publicGroupList": [
        {"id": 1, "name": "Infrastructure", "weight": 1, "monitorList": [
            {"id": 1, "name": "Mail", "type": "http"}, {"id": 2, "name": "VPN", "type": "port"}]},
        {"id": 2, "name": "Tests", "weight": 2, "monitorList": [{"id": 3, "name": "Recette"}]}
    ], "maintenanceList": []}"#;

fn text(key: &str) -> String {
    I18n::new(LANGUAGES[0].code).t(key).to_string()
}

fn plural(key: &str, n: usize) -> String {
    I18n::new(LANGUAGES[0].code).plural(key, n)
}

async fn spawn_with_publisher() -> TestApp {
    let app = TestApp::spawn().await;
    app.create_user(
        PUBLISHER_EMAIL,
        PUBLISHER_PASSWORD,
        "Editor",
        Role::Publisher,
    )
    .await;
    app.login(PUBLISHER_EMAIL, PUBLISHER_PASSWORD).await;
    app
}

/// A Kuma that serves the page `infra` and nothing else; its address in a
/// browser.
async fn fake_kuma() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("address");
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut request = [0u8; 4096];
                let read = stream.read(&mut request).await.unwrap_or(0);
                let head = String::from_utf8_lossy(&request[..read]);
                let (status, body) = if head.starts_with("GET /api/status-page/infra ") {
                    ("200 OK", PAGE)
                } else {
                    (
                        "404 Not Found",
                        r#"{"status":"fail","msg":"Status Page Not Found"}"#,
                    )
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}/status")
}

async fn service_names(app: &TestApp) -> Vec<String> {
    ServiceRepository::list_all(&app.pool)
        .await
        .expect("services")
        .into_iter()
        .map(|service| service.name)
        .collect()
}

#[tokio::test]
async fn a_publisher_imports_the_ticked_groups_of_a_page() {
    let app = spawn_with_publisher().await;
    ServiceRepository::create(&app.pool, "Recette", "recette", None, None, None)
        .await
        .expect("service");
    let page = format!("{}/infra", fake_kuma().await);
    let csrf = app.csrf_from("/services/import").await;

    let (status, body, _) = app
        .post_form("/services/import/read", &csrf, &[("address", &page)])
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Infrastructure") && body.contains("VPN"));
    assert!(body.contains(&plural("kuma_import.to_import", 2)));
    assert!(body.contains(&plural("kuma_import.existing", 1)));

    let (status, _, location) = app
        .post_form(
            "/services/import",
            &csrf,
            &[("address", &page), ("groups", "Infrastructure")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location.as_deref(), Some("/services?imported=2&folders=1"));
    assert_eq!(service_names(&app).await, ["Recette", "Mail", "VPN"]);
    let folders = FolderRepository::list(&app.pool).await.expect("folders");
    assert_eq!(folders[0].name, "Infrastructure");

    let (_, services) = app.get("/services?imported=2&folders=1").await;
    assert!(services.contains(&plural("services.imported", 2)));
    assert!(services.contains(&plural("services.folders_created", 1)));
    let (_, again) = app.get("/services/import").await;
    assert!(
        again.contains(&format!("value=\"{page}\"")),
        "the URL is kept"
    );
}

#[tokio::test]
async fn an_import_with_nothing_ticked_creates_nothing() {
    let app = spawn_with_publisher().await;
    let page = format!("{}/infra", fake_kuma().await);
    let csrf = app.csrf_from("/services/import").await;
    let (status, body, _) = app
        .post_form("/services/import", &csrf, &[("address", &page)])
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(&text("kuma_import.tick_one")));
    assert!(service_names(&app).await.is_empty());
}

#[tokio::test]
async fn a_page_that_cannot_be_read_says_why() {
    let app = spawn_with_publisher().await;
    let kuma = fake_kuma().await;
    let csrf = app.csrf_from("/services/import").await;
    for (address, key) in [
        (format!("{kuma}/missing"), "kuma_import.not_found"),
        (
            kuma.replace("/status", "/dashboard"),
            "validation.kuma_url_invalid",
        ),
        (String::new(), "validation.kuma_url_required"),
    ] {
        let (status, body, _) = app
            .post_form("/services/import/read", &csrf, &[("address", &address)])
            .await;
        assert_eq!(status, StatusCode::OK, "{address}");
        let message = text(key).replace('\'', "&#39;");
        assert!(body.contains(&message), "{address}: {key}");
    }
}

#[tokio::test]
async fn a_reader_cannot_import() {
    let app = TestApp::spawn().await;
    app.create_user(
        "reader@test.com",
        "reader_password_12",
        "Reader",
        Role::Reader,
    )
    .await;
    app.login("reader@test.com", "reader_password_12").await;
    let (status, _) = app.get("/services/import").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
