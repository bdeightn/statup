//! Who sees which service: a restricted service, and the events on it,
//! leave no trace for a visitor or a reader outside its group.

mod common;

use common::TestApp;
use statup::models::{CreateEventInput, Kind, Role, Severity, Visibility};
use statup::repositories::{GroupRepository, ServiceRepository};
use statup::services::EventService;

const PASSWORD: &str = "correct horse battery staple";

/// Names no other text of the page could contain.
const RESTRICTED: &str = "Coffre Zephyr";
const RESTRICTED_INCIDENT: &str = "Panne Quartz";
const OPEN: &str = "Intranet";

/// A public instance with an open service and one restricted to the
/// group "Paie", down under a critical incident. `inside` is a reader of
/// the group, `outside` a reader who is not.
struct Fixture {
    app: TestApp,
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
        ServiceRepository::create(pool, OPEN, "intranet", None, None, None)
            .await
            .unwrap();
        let restricted = ServiceRepository::create(pool, RESTRICTED, "coffre", None, None, None)
            .await
            .unwrap()
            .id;
        ServiceRepository::set_visibility(pool, restricted, Visibility::Groups, &[group])
            .await
            .unwrap();
        EventService::create(pool, incident(RESTRICTED_INCIDENT, admin, vec![restricted]))
            .await
            .unwrap();
        Self { app }
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
