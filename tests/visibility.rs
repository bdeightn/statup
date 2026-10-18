//! Who sees which service: a restricted service, and the events on it,
//! leave no trace for a visitor or a reader outside its group.

mod common;

use chrono::{Duration, Utc};
use common::TestApp;
use statup::models::{Category, CreateEventInput, Kind, Role, Severity, Visibility};
use statup::repositories::{GroupRepository, ServiceRepository};
use statup::services::EventService;

const PASSWORD: &str = "correct horse battery staple";

/// Names no other text of the page could contain.
const RESTRICTED: &str = "Coffre Zephyr";
const RESTRICTED_INCIDENT: &str = "Panne Quartz";
const OPEN: &str = "Intranet";
const MIXED_INCIDENT: &str = "Lenteur mixte";
const MAINTENANCE: &str = "Opale-33";
const ANNOUNCEMENT: &str = "Nouveautes Cobalt";

/// A public instance with an open service and one restricted to the
/// group "Paie", down under a critical incident. `inside` is a reader of
/// the group, `outside` a reader who is not. A second incident touches both
/// services, a maintenance is announced on the restricted one, and an
/// announcement that names no service follows it.
struct Fixture {
    app: TestApp,
    open_service: i64,
    restricted_service: i64,
    restricted_incident: i64,
    mixed_incident: i64,
    maintenance: i64,
    announcement: i64,
}

impl Fixture {
    async fn new() -> Self {
        let app = TestApp::spawn_public().await;
        let admin = app
            .create_user("admin@x.com", PASSWORD, "Admin", Role::Admin)
            .await;
        let inside = app
            .create_user("inside@x.com", PASSWORD, "Inside", Role::Reader)
            .await;
        app.create_user("outside@x.com", PASSWORD, "Outside", Role::Reader)
            .await;
        let pool = &app.pool;
        let group = GroupRepository::create(pool, "Paie").await.unwrap();
        GroupRepository::set_member_groups(pool, inside, &[group])
            .await
            .unwrap();
        let open = ServiceRepository::create(pool, OPEN, "intranet", None, None, None)
            .await
            .unwrap()
            .id;
        let restricted = ServiceRepository::create(pool, RESTRICTED, "coffre", None, None, None)
            .await
            .unwrap()
            .id;
        ServiceRepository::set_visibility(pool, restricted, Visibility::Groups, &[group])
            .await
            .unwrap();
        let restricted_incident =
            EventService::create(pool, incident(RESTRICTED_INCIDENT, admin, vec![restricted]))
                .await
                .unwrap()
                .id;
        let mixed_incident = EventService::create(
            pool,
            incident(MIXED_INCIDENT, admin, vec![open, restricted]),
        )
        .await
        .unwrap()
        .id;
        let maintenance = EventService::create(pool, maintenance(admin, vec![restricted]))
            .await
            .unwrap()
            .id;
        let announcement = EventService::create(pool, announcement(admin, maintenance))
            .await
            .unwrap()
            .id;
        Self {
            app,
            open_service: open,
            restricted_service: restricted,
            restricted_incident,
            mixed_incident,
            maintenance,
            announcement,
        }
    }

    /// Reads `path` as a visitor, or as `email` once signed in. A fixture
    /// signs in once: a second sign-in would find the first session open.
    async fn status_for(&self, path: &str, email: Option<&str>) -> (u16, String) {
        if let Some(email) = email {
            self.app.login(email, PASSWORD).await;
        }
        let (status, body) = self.app.get(path).await;
        (status.as_u16(), body)
    }

    /// The home page, its live refresh and the events list, as one text.
    async fn read_pages(&self) -> String {
        let mut text = String::new();
        for path in ["/", "/events"] {
            let (status, body) = self.app.get(path).await;
            assert!(status.is_success(), "{path}: {status}");
            text.push_str(&body);
        }
        let live = self
            .app
            .client
            .get(self.app.url("/"))
            .header("HX-Request", "true")
            .send()
            .await
            .unwrap();
        text.push_str(&live.text().await.unwrap());
        text
    }
}

fn incident(title: &str, author_id: i64, service_ids: Vec<i64>) -> CreateEventInput {
    CreateEventInput {
        kind: Kind::Incident,
        severity: Some(Severity::Critical),
        planned: false,
        category: None,
        title: title.to_string(),
        description: String::new(),
        planned_start: None,
        planned_end: None,
        started_at: None,
        opening_step: None,
        keeps_services_up: false,
        service_ids,
        follows_event_id: None,
        author_id,
        next_update: None,
    }
}

fn maintenance(author_id: i64, service_ids: Vec<i64>) -> CreateEventInput {
    CreateEventInput {
        kind: Kind::Maintenance,
        severity: None,
        planned: true,
        planned_start: Some(Utc::now() + Duration::days(2)),
        planned_end: Some(Utc::now() + Duration::days(2) + Duration::hours(1)),
        ..incident(MAINTENANCE, author_id, service_ids)
    }
}

fn announcement(author_id: i64, follows: i64) -> CreateEventInput {
    CreateEventInput {
        kind: Kind::Publication,
        severity: None,
        category: Some(Category::Info),
        follows_event_id: Some(follows),
        ..incident(ANNOUNCEMENT, author_id, Vec::new())
    }
}

fn assert_hidden(text: &str) {
    assert!(text.contains(OPEN), "the open service should show");
    assert!(!text.contains(RESTRICTED), "the restricted service leaked");
    assert!(!text.contains(RESTRICTED_INCIDENT), "its incident leaked");
}

fn assert_shown(text: &str) {
    assert!(text.contains(RESTRICTED));
    assert!(text.contains(RESTRICTED_INCIDENT));
}

#[tokio::test]
async fn a_visitor_sees_no_trace_of_a_restricted_service() {
    let fixture = Fixture::new().await;
    assert_hidden(&fixture.read_pages().await);
}

#[tokio::test]
async fn a_reader_outside_the_group_sees_no_trace_of_it() {
    let fixture = Fixture::new().await;
    fixture.app.login("outside@x.com", PASSWORD).await;
    assert_hidden(&fixture.read_pages().await);
}

#[tokio::test]
async fn a_reader_of_the_group_sees_it_and_its_incident() {
    let fixture = Fixture::new().await;
    fixture.app.login("inside@x.com", PASSWORD).await;
    assert_shown(&fixture.read_pages().await);
}

#[tokio::test]
async fn an_administrator_sees_everything() {
    let fixture = Fixture::new().await;
    fixture.app.login("admin@x.com", PASSWORD).await;
    assert_shown(&fixture.read_pages().await);
}

/// The page and the panel of a restricted incident, read as `email`.
async fn assert_restricted_event_status(email: Option<&str>, expected: u16) {
    let fixture = Fixture::new().await;
    if let Some(email) = email {
        fixture.app.login(email, PASSWORD).await;
    }
    let page = format!("/events/{}", fixture.restricted_incident);
    for path in [page.clone(), format!("{page}/drawer")] {
        assert_eq!(fixture.app.get(&path).await.0.as_u16(), expected, "{path}");
    }
}

/// The side panel of the restricted service, read as `email`.
async fn assert_restricted_service_panel_status(email: Option<&str>, expected: u16) {
    let fixture = Fixture::new().await;
    if let Some(email) = email {
        fixture.app.login(email, PASSWORD).await;
    }
    let panel = format!("/services/{}/drawer", fixture.restricted_service);
    assert_eq!(fixture.app.get(&panel).await.0.as_u16(), expected);
}

#[tokio::test]
async fn a_restricted_event_is_missing_for_a_visitor() {
    assert_restricted_event_status(None, 404).await;
}

#[tokio::test]
async fn a_restricted_event_is_missing_for_a_reader_outside_the_group() {
    assert_restricted_event_status(Some("outside@x.com"), 404).await;
}

#[tokio::test]
async fn a_restricted_event_opens_for_a_reader_of_the_group() {
    assert_restricted_event_status(Some("inside@x.com"), 200).await;
}

#[tokio::test]
async fn a_restricted_event_opens_for_an_administrator() {
    assert_restricted_event_status(Some("admin@x.com"), 200).await;
}

#[tokio::test]
async fn a_restricted_service_panel_is_missing_for_a_visitor() {
    assert_restricted_service_panel_status(None, 404).await;
}

#[tokio::test]
async fn a_restricted_service_panel_is_missing_for_a_reader_outside_the_group() {
    assert_restricted_service_panel_status(Some("outside@x.com"), 404).await;
}

#[tokio::test]
async fn a_restricted_service_panel_opens_for_a_reader_of_the_group() {
    assert_restricted_service_panel_status(Some("inside@x.com"), 200).await;
}

#[tokio::test]
async fn a_mixed_event_names_only_the_services_the_visitor_sees() {
    let fixture = Fixture::new().await;
    let page = format!("/events/{}", fixture.mixed_incident);
    let drawer = format!("{page}/drawer?service={}", fixture.restricted_service);
    let (status, body) = fixture.status_for(&page, None).await;
    assert_eq!(status, 200);
    assert!(body.contains(MIXED_INCIDENT));
    assert!(
        !body.contains(RESTRICTED),
        "the page named the hidden service"
    );
    let (status, body) = fixture.status_for(&drawer, None).await;
    assert_eq!(status, 200);
    assert!(
        !body.contains(RESTRICTED),
        "the panel named the hidden service"
    );
}

#[tokio::test]
async fn an_open_service_panel_lists_a_mixed_event_without_the_hidden_service() {
    let fixture = Fixture::new().await;
    let panel = format!("/services/{}/drawer", fixture.open_service);
    let (status, body) = fixture.status_for(&panel, None).await;
    assert_eq!(status, 200);
    assert!(body.contains(MIXED_INCIDENT));
    assert!(
        !body.contains(RESTRICTED),
        "the panel named the hidden service"
    );
}

#[tokio::test]
async fn an_announcement_does_not_link_a_maintenance_hidden_from_a_visitor() {
    let fixture = Fixture::new().await;
    let page = format!("/events/{}", fixture.announcement);
    let (status, body) = fixture.status_for(&page, None).await;
    assert_eq!(status, 200);
    assert!(body.contains(ANNOUNCEMENT));
    assert!(!body.contains(&format!("/events/{}\"", fixture.maintenance)));
    assert!(!body.contains(MAINTENANCE));
}

#[tokio::test]
async fn an_announcement_links_a_maintenance_to_a_reader_of_the_group() {
    let fixture = Fixture::new().await;
    let page = format!("/events/{}", fixture.announcement);
    let (status, body) = fixture.status_for(&page, Some("inside@x.com")).await;
    assert_eq!(status, 200);
    assert!(body.contains(&format!("/events/{}\"", fixture.maintenance)));
    assert!(body.contains(MAINTENANCE));
}

#[tokio::test]
async fn the_feed_shows_what_a_visitor_sees_even_to_an_administrator() {
    let fixture = Fixture::new().await;
    let (status, body) = fixture.status_for("/feed", Some("admin@x.com")).await;
    assert_eq!(status, 200);
    assert!(body.contains(MIXED_INCIDENT));
    assert!(!body.contains(RESTRICTED));
    assert!(!body.contains(RESTRICTED_INCIDENT));
}
