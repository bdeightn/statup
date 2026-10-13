//! The bell of the masthead: who has one, what it counts, and how opening
//! it marks the notices seen.

mod common;

use reqwest::StatusCode;

use common::TestApp;
use statup::models::{NoticeKind, NoticeSubject, Role};
use statup::repositories::{MemberNoticeRepository, ServiceRepository};

async fn signed_in(app: &TestApp, role: Role) -> i64 {
    let id = app
        .create_user("member@example.com", "member_pass_12", "Member", role)
        .await;
    app.login("member@example.com", "member_pass_12").await;
    id
}

async fn outage_notice(app: &TestApp, user_id: i64) {
    let service = ServiceRepository::create(&app.pool, "VPN", "vpn", None, None, None)
        .await
        .unwrap();
    let subject = NoticeSubject {
        service_id: Some(service.id),
        ..NoticeSubject::default()
    };
    MemberNoticeRepository::add_for_member(&app.pool, user_id, NoticeKind::OutageDetected, subject)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_publisher_sees_the_bell_count_its_unread_notices() {
    let app = TestApp::spawn().await;
    let editor = signed_in(&app, Role::Publisher).await;
    outage_notice(&app, editor).await;

    let (_, body) = app.get("/").await;

    assert!(body.contains("data-bell"));
    assert!(body.contains("data-bell-count>1<"));
    assert!(body.contains("Panne détectée\u{a0}: VPN"));
}

#[tokio::test]
async fn a_reader_has_no_bell_nor_notices_page() {
    let app = TestApp::spawn().await;
    signed_in(&app, Role::Reader).await;

    let (_, body) = app.get("/").await;
    assert!(!body.contains("data-bell"));

    let (status, _) = app.get("/notices").await;
    assert_ne!(status, StatusCode::OK);
}

#[tokio::test]
async fn opening_the_bell_marks_its_notices_seen() {
    let app = TestApp::spawn().await;
    let editor = signed_in(&app, Role::Publisher).await;
    outage_notice(&app, editor).await;
    let csrf = app.csrf_from("/").await;

    let response = app
        .client
        .post(app.url("/notices/seen"))
        .header("x-csrf-token", &csrf)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let (_, body) = app.get("/notices").await;
    assert!(!body.contains("data-bell-count"));
    assert!(
        body.contains("Panne détectée\u{a0}: VPN"),
        "the page still lists it"
    );
}
