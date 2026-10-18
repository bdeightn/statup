//! Who sees which service: a restricted service, and the events on it,
//! leave no trace for a visitor or a reader outside its group.

mod common;

use chrono::{Duration, Utc};
use common::TestApp;
use sqlx::SqlitePool;
use statup::i18n::{I18n, LANGUAGES};
use statup::models::{Category, CreateEventInput, Kind, Lifecycle, Role, Severity, Visibility};
use statup::repositories::{GroupRepository, ServiceRepository, UserRepository};
use statup::services::{EventService, ServiceService};

const PASSWORD: &str = "correct horse battery staple";

/// Names no other text of the page could contain.
const RESTRICTED: &str = "Coffre Zephyr";
const RESTRICTED_SLUG: &str = "coffre";
const RESTRICTED_INCIDENT: &str = "Panne Quartz";
const OPEN: &str = "Intranet";
const MIXED_INCIDENT: &str = "Lenteur mixte";
const MAINTENANCE: &str = "Opale-33";
const COMPLETED_MAINTENANCE: &str = "Basalte-12";
const UPDATE_TEXT: &str = "Xylo-55";
const MEMBERS_SERVICE: &str = "Annuaire K2";
const MEMBERS_SLUG: &str = "annuaire";
const ANNOUNCEMENT: &str = "Nouveautes Cobalt";

/// A public instance with an open service, one for signed-in members and
/// one restricted to the group "Paie", down under a critical incident that
/// carries an update, and where a check found an outage. `inside` is a reader
/// of the group, `outside` a reader who is not, `publisher` publishes. A second
/// incident touches both services, a maintenance is announced on the
/// restricted one and another is over, and an announcement that names no
/// service follows the first.
struct Fixture {
    app: TestApp,
    open_service: i64,
    restricted_service: i64,
    members_service: i64,
    restricted_incident: i64,
    mixed_incident: i64,
    maintenance: i64,
    completed_maintenance: i64,
    announcement: i64,
}

/// The events of a [`Fixture`].
struct Events {
    restricted_incident: i64,
    mixed_incident: i64,
    maintenance: i64,
    completed_maintenance: i64,
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
        app.create_user("publisher@x.com", PASSWORD, "Publisher", Role::Publisher)
            .await;
        let pool = &app.pool;
        let group = GroupRepository::create(pool, "Paie").await.unwrap();
        GroupRepository::add_members(pool, group, &[inside])
            .await
            .unwrap();
        let (open, restricted, members) = seed_services(pool, group).await;
        let events = seed_events(pool, admin, open, restricted).await;
        Self {
            app,
            open_service: open,
            restricted_service: restricted,
            members_service: members,
            restricted_incident: events.restricted_incident,
            mixed_incident: events.mixed_incident,
            maintenance: events.maintenance,
            completed_maintenance: events.completed_maintenance,
            announcement: events.announcement,
        }
    }

    /// Puts every member's last visit after the history a hidden event is
    /// not part of: the open incident is older than the visit, the events on
    /// the restricted service are newer. Only those can then count as unread.
    async fn visit_before_the_hidden_events(&self) {
        sqlx::query("UPDATE users SET last_seen_at = datetime('now', '-1 hour')")
            .execute(&self.app.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE events SET created_at = datetime('now', '-2 hours') WHERE id = ?")
            .bind(self.mixed_incident)
            .execute(&self.app.pool)
            .await
            .unwrap();
    }

    /// The ids of the events a reader outside the group cannot see.
    fn hidden_events(&self) -> [i64; 3] {
        [
            self.restricted_incident,
            self.maintenance,
            self.completed_maintenance,
        ]
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
        let (status, live) = self.app.get_htmx("/").await;
        assert!(status.is_success(), "/ (htmx): {status}");
        text.push_str(&live);
        text
    }
}

async fn seed_services(pool: &SqlitePool, group: i64) -> (i64, i64, i64) {
    let open = ServiceRepository::create(pool, OPEN, "intranet", None, None, None)
        .await
        .unwrap()
        .id;
    let restricted = ServiceRepository::create(pool, RESTRICTED, RESTRICTED_SLUG, None, None, None)
        .await
        .unwrap()
        .id;
    ServiceRepository::set_visibility(pool, restricted, Visibility::Groups, &[group])
        .await
        .unwrap();
    let members = ServiceRepository::create(pool, MEMBERS_SERVICE, MEMBERS_SLUG, None, None, None)
        .await
        .unwrap()
        .id;
    ServiceRepository::set_visibility(pool, members, Visibility::Members, &[])
        .await
        .unwrap();
    (open, restricted, members)
}

async fn seed_events(pool: &SqlitePool, admin: i64, open: i64, restricted: i64) -> Events {
    let create = |input| async move { EventService::create(pool, input).await.unwrap().id };
    let restricted_incident = create(incident(RESTRICTED_INCIDENT, admin, vec![restricted])).await;
    let mixed_incident = create(incident(MIXED_INCIDENT, admin, vec![open, restricted])).await;
    let announced = create(maintenance(MAINTENANCE, admin, vec![restricted])).await;
    let completed_maintenance =
        create(maintenance(COMPLETED_MAINTENANCE, admin, vec![restricted])).await;
    let announcement = create(announcement(admin, announced)).await;

    let author = UserRepository::find_by_id(pool, admin)
        .await
        .unwrap()
        .unwrap();
    EventService::post_update(pool, restricted_incident, UPDATE_TEXT, None, None, &author)
        .await
        .unwrap();
    EventService::post_update(
        pool,
        completed_maintenance,
        "",
        Some(Lifecycle::Completed),
        None,
        &author,
    )
    .await
    .unwrap();
    // As the monitoring does when checks fail, then the status follows.
    ServiceRepository::mark_detected_down(pool, restricted, Utc::now() - Duration::minutes(20))
        .await
        .unwrap();
    ServiceService::recalculate_status(pool, restricted)
        .await
        .unwrap();
    Events {
        restricted_incident,
        mixed_incident,
        maintenance: announced,
        completed_maintenance,
        announcement,
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

fn maintenance(title: &str, author_id: i64, service_ids: Vec<i64>) -> CreateEventInput {
    CreateEventInput {
        kind: Kind::Maintenance,
        severity: None,
        planned: true,
        planned_start: Some(Utc::now() + Duration::days(2)),
        planned_end: Some(Utc::now() + Duration::days(2) + Duration::hours(1)),
        ..incident(title, author_id, service_ids)
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

/// One answer of the sweep, with the route that gave it.
struct Page {
    route: String,
    body: String,
}

/// What follows a link to an event in markup: a quote or a slash (page,
/// panel), a query, or the end of a tag (feed). Checking the terminator keeps
/// `/events/1` from matching inside `/events/12`.
const LINK_ENDS: [char; 4] = ['"', '/', '?', '<'];

/// The text of the unread badge in the masthead, if there is one.
const UNREAD_MARKER: &str = r#"class="unread" aria-hidden="true">"#;

impl Fixture {
    /// The routes that draw events or services, and whether htmx also
    /// refreshes each one (then it is requested a second time as it does).
    fn sweep_routes(&self, signed_in: bool) -> Vec<(String, bool)> {
        let mixed = self.mixed_incident;
        let mut routes = vec![
            ("/".to_string(), true),
            ("/events".to_string(), true),
            ("/events?q=Panne".to_string(), true),
            (
                format!("/events?service_id={}", self.restricted_service),
                true,
            ),
            ("/events?kind=incident".to_string(), false),
            ("/events?kind=maintenance".to_string(), false),
            ("/events?lifecycle=active".to_string(), false),
            ("/events?lifecycle=closed".to_string(), false),
            (format!("/events/{mixed}"), false),
            (format!("/events/{mixed}/drawer"), false),
            (
                format!("/events/{mixed}/drawer?service={}", self.restricted_service),
                false,
            ),
            (format!("/events/{}", self.announcement), false),
            (format!("/services/{}/drawer", self.open_service), false),
            ("/feed".to_string(), false),
            ("/subscribe".to_string(), false),
        ];
        if signed_in {
            routes.push(("/profile".to_string(), false));
        }
        routes
    }

    /// Reads every route of the sweep, each of them answering 200.
    async fn sweep(&self, signed_in: bool) -> Vec<Page> {
        let mut pages = Vec::new();
        for (route, refreshed) in self.sweep_routes(signed_in) {
            let (status, body) = self.app.get(&route).await;
            assert_eq!(status.as_u16(), 200, "{route}");
            pages.push(Page {
                route: route.clone(),
                body,
            });
            if refreshed {
                let (status, body) = self.app.get_htmx(&route).await;
                assert_eq!(status.as_u16(), 200, "{route} (htmx)");
                pages.push(Page {
                    route: format!("{route} (htmx)"),
                    body,
                });
            }
        }
        pages
    }

    /// What no reader outside the group may find on any page.
    fn hidden_strings(&self) -> Vec<String> {
        let mut strings: Vec<String> = [
            RESTRICTED_INCIDENT,
            MAINTENANCE,
            COMPLETED_MAINTENANCE,
            UPDATE_TEXT,
            RESTRICTED,
            // A search wraps the words it found in markup, so a hidden title
            // can reach a page in pieces.
            "Quartz",
            "Opale",
            "Basalte",
        ]
        .map(String::from)
        .to_vec();
        strings.push(format!("service-{RESTRICTED_SLUG}"));
        for id in self.hidden_events() {
            strings.extend(LINK_ENDS.map(|end| format!("/events/{id}{end}")));
        }
        strings
    }

    /// The page and the panel of every hidden event, and the panel of the
    /// restricted service, all answering `expected`.
    async fn assert_hidden_routes_answer(&self, expected: u16) {
        let mut routes = vec![format!("/services/{}/drawer", self.restricted_service)];
        for id in self.hidden_events() {
            routes.push(format!("/events/{id}"));
            routes.push(format!("/events/{id}/drawer"));
        }
        for route in routes {
            assert_eq!(self.app.get(&route).await.0.as_u16(), expected, "{route}");
        }
    }

    /// The count in the masthead badge, read on the profile page: unlike the
    /// home page and the events list, it does not mark the member as seen.
    async fn unread_badge(&self) -> Option<String> {
        let (status, body) = self.app.get("/profile").await;
        assert_eq!(status.as_u16(), 200, "/profile");
        let after = body.split(UNREAD_MARKER).nth(1)?;
        after.split('<').next().map(String::from)
    }

    /// A fixture whose member `email` is signed in, whose last visit came
    /// before the events of the restricted service.
    async fn signed_in(email: &str) -> Self {
        let fixture = Self::new().await;
        fixture.visit_before_the_hidden_events().await;
        fixture.app.login(email, PASSWORD).await;
        fixture
    }
}

/// The banner headline for `n` disrupted services, in the language a client
/// that asks for none gets. Counting a hidden service would betray it.
fn disrupted(n: usize) -> String {
    I18n::new(LANGUAGES[0].code).plural("banner.disrupted", n)
}

/// Fails naming the route and the string when a page holds a forbidden text.
fn forbid<'a>(pages: impl IntoIterator<Item = &'a Page>, strings: &[String]) {
    for page in pages {
        for text in strings {
            assert!(
                !page.body.contains(text.as_str()),
                "{} leaked {text:?}",
                page.route
            );
        }
    }
}

/// Fails unless the page read on `route` holds `text`.
fn assert_shows(pages: &[Page], route: &str, text: &str) {
    let page = pages.iter().find(|page| page.route == route);
    let page = page.unwrap_or_else(|| panic!("{route} was not read"));
    assert!(page.body.contains(text), "{route} does not show {text:?}");
}

fn on_route<'a>(pages: &'a [Page], route: &'a str) -> impl Iterator<Item = &'a Page> {
    pages.iter().filter(move |page| page.route == route)
}

/// A member who sees the restricted service: the sweep shows it and its
/// events, and only the feed, which is the visitor's view, keeps it out.
async fn assert_sees_the_restricted_service(fixture: &Fixture) {
    let pages = fixture.sweep(true).await;
    assert_shows(&pages, "/", RESTRICTED);
    assert_shows(&pages, "/", &disrupted(2));
    for title in [RESTRICTED_INCIDENT, MAINTENANCE, COMPLETED_MAINTENANCE] {
        assert_shows(&pages, "/events", title);
    }
    assert_shows(&pages, "/events?q=Panne", "Quartz");
    forbid(on_route(&pages, "/feed"), &fixture.hidden_strings());
    fixture.assert_hidden_routes_answer(200).await;
}

#[tokio::test]
async fn the_sweep_finds_no_trace_of_hidden_services_for_a_visitor() {
    let fixture = Fixture::new().await;
    let pages = fixture.sweep(false).await;
    assert_shows(&pages, "/", OPEN);
    assert_shows(&pages, "/", &disrupted(1));
    let mut forbidden = fixture.hidden_strings();
    forbidden.push(MEMBERS_SERVICE.to_string());
    forbidden.push(format!("service-{MEMBERS_SLUG}"));
    forbid(&pages, &forbidden);
    fixture.assert_hidden_routes_answer(404).await;
    let members_panel = format!("/services/{}/drawer", fixture.members_service);
    assert_eq!(fixture.app.get(&members_panel).await.0.as_u16(), 404);
}

#[tokio::test]
async fn the_sweep_finds_no_trace_of_the_group_service_for_a_reader_outside_it() {
    let fixture = Fixture::signed_in("outside@x.com").await;
    assert_eq!(fixture.unread_badge().await, None, "hidden events counted");
    let pages = fixture.sweep(true).await;
    assert_shows(&pages, "/", OPEN);
    assert_shows(&pages, "/", MEMBERS_SERVICE);
    assert_shows(&pages, "/", &disrupted(1));
    forbid(&pages, &fixture.hidden_strings());
    fixture.assert_hidden_routes_answer(404).await;
}

#[tokio::test]
async fn the_sweep_shows_the_group_service_to_a_reader_of_the_group() {
    let fixture = Fixture::signed_in("inside@x.com").await;
    // The three events on the restricted service came after the last visit.
    assert_eq!(fixture.unread_badge().await.as_deref(), Some("3"));
    assert_sees_the_restricted_service(&fixture).await;
}

#[tokio::test]
async fn the_sweep_shows_the_group_service_to_a_publisher() {
    let fixture = Fixture::signed_in("publisher@x.com").await;
    assert_sees_the_restricted_service(&fixture).await;
}

/// The form of a service reserves it, puts it back for everyone, and asks
/// when nothing is ticked.
#[tokio::test]
async fn an_editor_keeps_a_service_for_a_group_from_its_form() {
    let app = TestApp::spawn_public().await;
    app.create_user("editor@x.com", PASSWORD, "Editor", Role::Publisher)
        .await;
    let group = GroupRepository::create(&app.pool, "Paie").await.unwrap();
    app.login("editor@x.com", PASSWORD).await;
    let i18n = I18n::new(LANGUAGES[0].code);
    let csrf = app.csrf_from("/services/new").await;
    let group_id = group.to_string();

    let fields = [
        ("name", "Coffre"),
        ("check_kind", "none"),
        ("visibility", "groups"),
        ("group_ids", group_id.as_str()),
    ];
    let (status, _, _) = app.post_form("/services/new", &csrf, &fields).await;
    assert_eq!(status.as_u16(), 303);
    let service = ServiceRepository::list_all(&app.pool)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "Coffre")
        .unwrap();
    assert_eq!(service.visibility, Visibility::Groups);
    let edit = format!("/services/{}/edit", service.id);
    let (_, form) = app.get(&edit).await;
    assert!(
        form.contains("data-visibility-summary>Paie</span>"),
        "the closed menu says who sees it"
    );
    let ticked = format!(r#"name="group_ids" value="{group}" class="checkbox" checked"#);
    assert!(form.contains(&ticked), "the form shows the group ticked");

    let nothing = [
        ("name", "Coffre"),
        ("check_kind", "none"),
        ("visibility", "groups"),
    ];
    let (status, body, _) = app.post_form(&edit, &csrf, &nothing).await;
    assert_eq!(status.as_u16(), 200);
    assert!(body.contains(i18n.t("validation.visibility_required")));
    let unchanged = ServiceRepository::find_by_id(&app.pool, service.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        unchanged.visibility,
        Visibility::Groups,
        "a refusal saves nothing"
    );

    let everyone = [
        ("name", "Coffre"),
        ("check_kind", "none"),
        ("visibility", "everyone"),
    ];
    let (status, _, _) = app.post_form(&edit, &csrf, &everyone).await;
    assert_eq!(status.as_u16(), 303);
    let opened = ServiceRepository::find_by_id(&app.pool, service.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(opened.visibility, Visibility::Everyone);
    assert_eq!(
        ServiceRepository::group_ids_of(&app.pool, service.id)
            .await
            .unwrap(),
        Vec::<i64>::new()
    );
}

/// On a page for members only, "signed-in members" would mean the same as
/// "everyone": the form leaves it out.
#[tokio::test]
async fn a_members_only_page_offers_no_signed_in_members_choice() {
    let app = TestApp::spawn().await;
    app.create_user("editor@x.com", PASSWORD, "Editor", Role::Publisher)
        .await;
    app.login("editor@x.com", PASSWORD).await;
    let (_, form) = app.get("/services/new").await;
    assert!(form.contains(r#"name="visibility" value="everyone""#));
    assert!(!form.contains(r#"name="visibility" value="members""#));
}
