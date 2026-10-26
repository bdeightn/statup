//! Integration tests for the API's guards: a key in the `Authorization`
//! header is the only proof, every refusal is JSON, a session opens
//! nothing, and the API shares the rate limit of the pages.

mod common;

use chrono::Utc;
use reqwest::StatusCode;
use serde_json::Value;
use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use common::TestApp;
use statup::models::{Category, CreateEventInput, Kind, Lifecycle, Role, Severity, User};
use statup::repositories::{ApiKeyRepository, EventRepository, ServiceRepository, UserRepository};
use statup::services::{ApiKeyService, EventService, NewApiKey};

const ADMIN_EMAIL: &str = "admin@test.com";
const ADMIN_PASSWORD: &str = "admin_password_12";
const SERVICES: &str = "/api/v1/services";
const EVENTS: &str = "/api/v1/events";

struct Answer {
    status: StatusCode,
    headers: reqwest::header::HeaderMap,
    body: Value,
}

async fn admin_key(app: &TestApp, name: &str) -> NewApiKey {
    let id = app
        .create_user(ADMIN_EMAIL, ADMIN_PASSWORD, "Admin", Role::Admin)
        .await;
    let creator = UserRepository::find_by_id(&app.pool, id)
        .await
        .unwrap()
        .expect("the admin exists");
    ApiKeyService::create(&app.pool, name, &creator)
        .await
        .expect("the key is made")
}

async fn call(app: &TestApp, path: &str, authorization: Option<&str>) -> Answer {
    let mut request = app.client.get(app.url(path));
    if let Some(value) = authorization {
        request = request.header("Authorization", value);
    }
    answer(request.send().await.expect("API request failed")).await
}

async fn answer(resp: reqwest::Response) -> Answer {
    let status = resp.status();
    let headers = resp.headers().clone();
    let text = resp.text().await.unwrap_or_default();
    let body = serde_json::from_str(&text).unwrap_or_else(|_| panic!("not JSON: {text}"));
    Answer {
        status,
        headers,
        body,
    }
}

fn bearer(key: &str) -> String {
    format!("Bearer {key}")
}

fn header<'a>(answer: &'a Answer, name: &str) -> Option<&'a str> {
    answer.headers.get(name).and_then(|v| v.to_str().ok())
}

fn error_code(answer: &Answer) -> &str {
    answer.body["error"]["code"].as_str().unwrap_or_default()
}

fn assert_refused(answer: &Answer) {
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(answer), "unauthorized");
    assert_eq!(header(answer, "www-authenticate"), Some("Bearer"));
    assert!(header(answer, "content-type").is_some_and(|v| v.starts_with("application/json")));
    assert_eq!(header(answer, "cache-control"), Some("no-store"));
}

#[tokio::test]
async fn a_key_in_use_reads_the_services_in_their_order() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;
    ServiceRepository::create(&app.pool, "Mail", "mail", Some("Outlook"), None, None)
        .await
        .unwrap();
    ServiceRepository::create(&app.pool, "VPN", "vpn", None, None, None)
        .await
        .unwrap();

    let answer = call(&app, SERVICES, Some(&bearer(&made.key))).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(header(&answer, "cache-control"), Some("no-store"));
    let services = answer.body["services"].as_array().expect("a list");
    let names: Vec<&str> = services.iter().filter_map(|s| s["name"].as_str()).collect();
    assert_eq!(names, ["Mail", "VPN"]);
    assert_eq!(services[0]["slug"], "mail");
    assert_eq!(services[0]["description"], "Outlook");
    assert_eq!(services[1]["description"], Value::Null);
    assert_eq!(services[0]["status"], "operational");
    assert_eq!(services[0]["manual_status"], "operational");
    assert_eq!(services[0]["monitored"], false);
    let updated_at = services[0]["updated_at"].as_str().unwrap_or_default();
    assert!(
        updated_at.ends_with('Z') && updated_at.len() == 20,
        "{updated_at}"
    );
}

async fn read(app: &TestApp, path: &str, key: &NewApiKey) -> Answer {
    call(app, path, Some(&bearer(&key.key))).await
}

async fn admin(app: &TestApp) -> User {
    UserRepository::find_by_email(&app.pool, ADMIN_EMAIL)
        .await
        .unwrap()
        .expect("the admin exists")
}

fn incident(title: &str, author: &User, service_ids: Vec<i64>) -> CreateEventInput {
    CreateEventInput {
        kind: Kind::Incident,
        severity: Some(Severity::Critical),
        planned: false,
        category: None,
        title: title.to_string(),
        description: "**Everything** is down".to_string(),
        planned_start: None,
        planned_end: None,
        started_at: None,
        opening_step: None,
        keeps_services_up: false,
        service_ids,
        follows_event_id: None,
        author_id: author.id,
        next_update: None,
    }
}

fn announcement(title: &str, author: &User) -> CreateEventInput {
    CreateEventInput {
        kind: Kind::Publication,
        severity: None,
        category: Some(Category::Info),
        ..incident(title, author, Vec::new())
    }
}

fn titles(answer: &Answer) -> Vec<&str> {
    answer.body["events"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|event| event["title"].as_str())
        .collect()
}

#[tokio::test]
async fn one_service_is_read_by_its_id() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;
    let mail = ServiceRepository::create(&app.pool, "Mail", "mail", Some("Outlook"), None, None)
        .await
        .unwrap();

    let answer = read(&app, &format!("{SERVICES}/{}", mail.id), &made).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.body["id"], mail.id);
    assert_eq!(answer.body["name"], "Mail");
    assert_eq!(answer.body["description"], "Outlook");
    assert_eq!(answer.body["status"], "operational");
    assert_eq!(answer.body["manual_status"], "operational");
    assert_eq!(answer.body["monitored"], false);

    let missing = read(&app, &format!("{SERVICES}/{}", mail.id + 100), &made).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert_eq!(error_code(&missing), "not_found");

    let malformed = read(&app, &format!("{SERVICES}/abc"), &made).await;
    assert_eq!(malformed.status, StatusCode::BAD_REQUEST);
    assert_eq!(error_code(&malformed), "invalid_data");
}

#[tokio::test]
async fn an_open_incident_shows_on_the_status_and_not_on_the_manual_status() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;
    let author = admin(&app).await;
    let mail = ServiceRepository::create(&app.pool, "Mail", "mail", None, None, None)
        .await
        .unwrap();
    EventService::create(&app.pool, incident("Mail is down", &author, vec![mail.id]))
        .await
        .unwrap();

    let answer = read(&app, &format!("{SERVICES}/{}", mail.id), &made).await;
    assert_eq!(answer.body["status"], "major_outage");
    assert_eq!(answer.body["manual_status"], "operational");
}

#[tokio::test]
async fn events_are_listed_newest_first_and_filtered() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;
    let author = admin(&app).await;
    let mail = ServiceRepository::create(&app.pool, "Mail", "mail", None, None, None)
        .await
        .unwrap();
    let vpn = ServiceRepository::create(&app.pool, "VPN", "vpn", None, None, None)
        .await
        .unwrap();
    let over = EventService::create(&app.pool, incident("Old outage", &author, vec![vpn.id]))
        .await
        .unwrap();
    EventService::post_update(
        &app.pool,
        over.id,
        "",
        Some(Lifecycle::Resolved),
        None,
        &author,
    )
    .await
    .unwrap();
    EventService::create(&app.pool, incident("Mail outage", &author, vec![mail.id]))
        .await
        .unwrap();
    EventService::create(&app.pool, announcement("New release", &author))
        .await
        .unwrap();

    let all = read(&app, EVENTS, &made).await;
    assert_eq!(all.status, StatusCode::OK);
    assert_eq!(header(&all, "cache-control"), Some("no-store"));
    assert_eq!(titles(&all), ["New release", "Mail outage", "Old outage"]);
    assert_eq!(all.body["total"], 3);
    let outage = &all.body["events"][1];
    assert_eq!(outage["kind"], "incident");
    assert_eq!(outage["severity"], "critical");
    assert_eq!(outage["category"], Value::Null);
    assert_eq!(outage["state"], "investigating");
    assert_eq!(outage["services"][0]["id"], mail.id);
    assert_eq!(outage["services"][0]["name"], "Mail");
    assert_eq!(all.body["events"][0]["state"], Value::Null);
    assert_eq!(all.body["events"][0]["category"], "info");
    assert!(outage.get("updates").is_none());

    let incidents = read(&app, &format!("{EVENTS}?kind=incident"), &made).await;
    assert_eq!(titles(&incidents), ["Mail outage", "Old outage"]);
    let open = read(&app, &format!("{EVENTS}?open=true"), &made).await;
    assert_eq!(titles(&open), ["Mail outage"]);
    let closed = read(&app, &format!("{EVENTS}?open=false"), &made).await;
    assert_eq!(titles(&closed), ["Old outage"]);

    let page = read(&app, &format!("{EVENTS}?limit=1&offset=1"), &made).await;
    assert_eq!(titles(&page), ["Mail outage"]);
    assert_eq!(page.body["total"], 3);
}

#[tokio::test]
async fn a_bad_filter_is_refused_and_a_page_out_of_range_is_clamped() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;
    let author = admin(&app).await;
    EventService::create(&app.pool, announcement("New release", &author))
        .await
        .unwrap();

    let unknown = read(&app, &format!("{EVENTS}?kind=nonsense"), &made).await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
    assert_eq!(error_code(&unknown), "invalid_data");

    for query in ["limit=0", "limit=1000", "offset=-5", "limit=-1&offset=-1"] {
        let answer = read(&app, &format!("{EVENTS}?{query}"), &made).await;
        assert_eq!(answer.status, StatusCode::OK, "{query}");
        assert_eq!(titles(&answer), ["New release"], "{query}");
    }
}

#[tokio::test]
async fn one_event_comes_with_its_services_and_updates() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;
    let author = admin(&app).await;
    let mail = ServiceRepository::create(&app.pool, "Mail", "mail", None, None, None)
        .await
        .unwrap();
    let vpn = ServiceRepository::create(&app.pool, "VPN", "vpn", None, None, None)
        .await
        .unwrap();
    let event = EventService::create(
        &app.pool,
        incident("Network outage", &author, vec![mail.id, vpn.id]),
    )
    .await
    .unwrap();
    EventRepository::add_update(&app.pool, event.id, "<p>Looking into it</p>", author.id)
        .await
        .unwrap();
    EventRepository::add_update(
        &app.pool,
        event.id,
        "<p>Back <strong>up</strong></p>",
        author.id,
    )
    .await
    .unwrap();

    let answer = read(&app, &format!("{EVENTS}/{}", event.id), &made).await;
    assert_eq!(answer.status, StatusCode::OK);
    let body = &answer.body;
    assert_eq!(body["id"], event.id);
    assert_eq!(body["title"], "Network outage");
    assert_eq!(body["description"], "**Everything** is down");
    assert_eq!(body["kind"], "incident");
    assert_eq!(body["state"], "investigating");
    let mut services: Vec<(i64, &str)> = body["services"]
        .as_array()
        .expect("services")
        .iter()
        .map(|s| (s["id"].as_i64().unwrap(), s["name"].as_str().unwrap()))
        .collect();
    services.sort_unstable();
    assert_eq!(services, [(mail.id, "Mail"), (vpn.id, "VPN")]);
    assert!(
        body["started_at"]
            .as_str()
            .is_some_and(|d| d.ends_with('Z'))
    );
    assert_eq!(body["ended_at"], Value::Null);

    let updates = body["updates"].as_array().expect("updates");
    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0]["message"], "Back up");
    assert_eq!(
        updates[0]["message_html"],
        "<p>Back <strong>up</strong></p>"
    );
    assert_eq!(updates[0]["author"], "Admin");
    assert_eq!(updates[1]["message"], "Looking into it");
    assert!(
        updates[0]["created_at"]
            .as_str()
            .is_some_and(|d| d.ends_with('Z'))
    );

    let missing = read(&app, &format!("{EVENTS}/{}", event.id + 100), &made).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert_eq!(error_code(&missing), "not_found");
}

#[tokio::test]
async fn every_read_endpoint_wants_a_key() {
    let app = TestApp::spawn().await;
    for path in [
        format!("{SERVICES}/1"),
        EVENTS.to_string(),
        format!("{EVENTS}/1"),
    ] {
        assert_refused(&call(&app, &path, None).await);
    }
}

#[tokio::test]
async fn a_use_of_the_key_is_recorded() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;
    let before = Utc::now() - chrono::Duration::seconds(1);

    call(&app, SERVICES, Some(&bearer(&made.key))).await;

    let holder = ApiKeyService::holder_of(&app.pool, &made.key)
        .await
        .unwrap()
        .expect("the key is in use");
    assert!(holder.last_used_at.is_some_and(|at| at >= before));
}

#[tokio::test]
async fn a_request_without_a_good_key_is_refused_in_json() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;
    let unknown = format!("stp_{}", "a".repeat(40));

    for authorization in [
        None,
        Some(bearer(&unknown)),
        Some(bearer("not-a-key")),
        Some("Bearer".to_string()),
        Some(made.key.clone()),
        Some(format!("Basic {}", made.key)),
    ] {
        assert_refused(&call(&app, SERVICES, authorization.as_deref()).await);
    }
}

#[tokio::test]
async fn a_key_in_the_address_is_never_read() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;

    for query in ["key", "api_key", "access_token", "token"] {
        let path = format!("{SERVICES}?{query}={}", made.key);
        assert_refused(&call(&app, &path, None).await);
    }
}

#[tokio::test]
async fn a_revoked_or_replaced_key_stops_at_once() {
    let app = TestApp::spawn().await;
    let revoked = admin_key(&app, "Zabbix").await;
    ApiKeyRepository::revoke(&app.pool, revoked.id, Utc::now())
        .await
        .unwrap();
    assert_refused(&call(&app, SERVICES, Some(&bearer(&revoked.key))).await);

    let creator = UserRepository::find_by_email(&app.pool, ADMIN_EMAIL)
        .await
        .unwrap()
        .expect("the admin exists");
    let old = ApiKeyService::create(&app.pool, "Grafana", &creator)
        .await
        .unwrap();
    let new = ApiKeyService::regenerate(&app.pool, old.id).await.unwrap();
    assert_refused(&call(&app, SERVICES, Some(&bearer(&old.key))).await);
    let answer = call(&app, SERVICES, Some(&bearer(&new.key))).await;
    assert_eq!(answer.status, StatusCode::OK);
}

/// A key belongs to the instance: disabling the admin who made it does not
/// cut a monitoring tool off without a word.
#[tokio::test]
async fn a_disabled_creator_leaves_the_key_working() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;
    sqlx::query("UPDATE users SET is_active = 0")
        .execute(&app.pool)
        .await
        .unwrap();

    let answer = call(&app, SERVICES, Some(&bearer(&made.key))).await;
    assert_eq!(answer.status, StatusCode::OK);
}

#[tokio::test]
async fn a_session_opens_nothing_and_the_api_sets_no_cookie() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;
    app.login(ADMIN_EMAIL, ADMIN_PASSWORD).await;
    assert!(app.session_cookie().is_some(), "the admin is signed in");
    let sessions = app.session_rows().await;

    let refused = call(&app, SERVICES, None).await;
    assert_refused(&refused);
    let allowed = call(&app, SERVICES, Some(&bearer(&made.key))).await;
    assert_eq!(allowed.status, StatusCode::OK);

    for answer in [&refused, &allowed] {
        assert!(answer.headers.get("set-cookie").is_none());
    }
    assert_eq!(app.session_rows().await, sessions);
}

#[tokio::test]
async fn an_address_or_a_method_the_api_does_not_have_answers_in_json() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;
    let key = bearer(&made.key);

    for path in ["/api/v1/nothing", "/api/v2/services", "/api"] {
        let answer = call(&app, path, Some(&key)).await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(error_code(&answer), "not_found");
    }

    let resp = app
        .client
        .delete(app.url(SERVICES))
        .header("Authorization", &key)
        .send()
        .await
        .unwrap();
    let answer = answer(resp).await;
    assert_eq!(answer.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(error_code(&answer), "method_not_allowed");
}

#[tokio::test]
async fn a_body_past_the_limit_is_refused_in_json() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;

    let resp = app
        .client
        .get(app.url(SERVICES))
        .header("Authorization", bearer(&made.key))
        .body(vec![b'a'; 65 * 1024])
        .send()
        .await
        .unwrap();
    let answer = answer(resp).await;
    assert_eq!(answer.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(error_code(&answer), "payload_too_large");
}

#[tokio::test]
async fn the_api_shares_the_rate_limit_of_the_pages() {
    let app = TestApp::spawn().await;
    let made = admin_key(&app, "Zabbix").await;
    let key = bearer(&made.key);

    // Together, 16 at a time: the budget refills every 600 ms, so a slow
    // sequence would never run out, and 130 connections at once overflow
    // the listen queue.
    let in_flight = Arc::new(Semaphore::new(16));
    let mut requests = JoinSet::new();
    for _ in 0..130 {
        let request = app
            .client
            .get(app.url(SERVICES))
            .header("Authorization", &key);
        let in_flight = Arc::clone(&in_flight);
        requests.spawn(async move {
            let _slot = in_flight.acquire_owned().await.expect("semaphore closed");
            answer(request.send().await.expect("request failed")).await
        });
    }
    let mut limited = Vec::new();
    while let Some(answer) = requests.join_next().await {
        let answer = answer.expect("request task failed");
        if answer.status == StatusCode::TOO_MANY_REQUESTS {
            limited.push(answer);
        }
    }

    let answer = limited
        .first()
        .expect("a burst past 100 requests is limited");
    assert_eq!(error_code(answer), "rate_limited");
    assert!(header(answer, "retry-after").is_some());
    let (status, _) = app.get("/login").await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "one budget per address"
    );
}
