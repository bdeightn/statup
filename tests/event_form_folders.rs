//! The "Services concerned" list of the event form: the services under their
//! folders, a folder box that checks them all, and a search past a few.

mod common;

use reqwest::StatusCode;

use common::TestApp;
use statup::models::Role;
use statup::repositories::{EventRepository, FolderRepository, ServiceRepository};

const PUBLISHER_EMAIL: &str = "editor@test.com";
const PUBLISHER_PASSWORD: &str = "editor_password_12";
const SEARCH_SCRIPT: &str = "js/list-filter.js";

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

async fn create_folder(app: &TestApp, name: &str) -> i64 {
    FolderRepository::create(&app.pool, name)
        .await
        .expect("failed to create folder")
}

async fn create_service(app: &TestApp, name: &str) -> i64 {
    ServiceRepository::create(&app.pool, name, &name.to_lowercase(), None, None, None)
        .await
        .expect("failed to create service")
        .id
}

/// A folder holding one new service per name, in that order.
async fn create_folder_with(app: &TestApp, name: &str, services: &[&str]) -> (i64, Vec<i64>) {
    let folder = create_folder(app, name).await;
    let mut ids = Vec::new();
    for service in services {
        ids.push(create_service(app, service).await);
    }
    FolderRepository::add_services(&app.pool, folder, &ids)
        .await
        .expect("failed to fill the folder");
    (folder, ids)
}

async fn page(app: &TestApp, path: &str) -> String {
    let (status, body) = app.get(path).await;
    assert_eq!(status, StatusCode::OK, "{path}");
    body
}

/// Where a marker sits in the page, which must show it.
fn at(body: &str, marker: &str) -> usize {
    body.find(marker)
        .unwrap_or_else(|| panic!("the page lacks {marker}"))
}

fn service_marker(id: i64) -> String {
    format!("name=\"service_ids\" value=\"{id}\"")
}

fn folder_marker(id: i64) -> String {
    format!("id=\"svc-folder-{id}\"")
}

/// The tag, from `<` to `>`, around this place in the page.
fn tag_at(body: &str, at: usize) -> &str {
    let start = body[..at].rfind('<').expect("an opening tag");
    let end = at + body[at..].find('>').expect("a closing bracket");
    &body[start..=end]
}

fn tag_around<'a>(body: &'a str, marker: &str) -> &'a str {
    tag_at(body, at(body, marker))
}

/// The tag of every folder box, in page order.
fn folder_box_tags(body: &str) -> Vec<&str> {
    body.match_indices("data-folder-box")
        .map(|(at, _)| tag_at(body, at))
        .collect()
}

/// The part of the page from a folder's own marker to the next section.
fn folder_section(body: &str, folder: i64) -> &str {
    let start = at(body, &folder_marker(folder));
    let rest = &body[start..];
    let end = rest.find("data-filter-group").unwrap_or(rest.len());
    &rest[..end]
}

async fn create_incident_with(app: &TestApp, service_ids: &[i64]) -> i64 {
    let csrf = app.csrf_from("/events/new").await;
    let ids: Vec<String> = service_ids.iter().map(i64::to_string).collect();
    let mut fields = vec![
        ("title", "Network incident"),
        ("description", "The network is slow"),
        ("kind", "incident"),
        ("severity", "critical"),
    ];
    fields.extend(ids.iter().map(|id| ("service_ids", id.as_str())));
    let (status, _, location) = app.post_form("/events/new", &csrf, &fields).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let path = location.expect("a redirect");
    path.split('?')
        .next()
        .and_then(|p| p.rsplit('/').next())
        .and_then(|id| id.parse().ok())
        .unwrap_or_else(|| panic!("no event id in {path}"))
}

#[tokio::test]
async fn without_folders_the_list_is_flat_in_the_chosen_order() {
    let app = spawn_with_publisher().await;
    let first = create_service(&app, "Mail").await;
    let second = create_service(&app, "Chat").await;
    let third = create_service(&app, "Files").await;
    ServiceRepository::reorder(&app.pool, &[third, first, second])
        .await
        .expect("failed to reorder");

    let body = page(&app, "/events/new").await;

    assert!(!body.contains("data-folder-pick"));
    assert!(!body.contains("data-folder-box"));
    let positions: Vec<usize> = [third, first, second]
        .iter()
        .map(|id| at(&body, &service_marker(*id)))
        .collect();
    assert!(positions.is_sorted(), "{positions:?}");
}

#[tokio::test]
async fn the_search_appears_only_past_eight_services() {
    let app = spawn_with_publisher().await;
    for n in 1..=8 {
        create_service(&app, &format!("Service {n}")).await;
    }

    let body = page(&app, "/events/new").await;
    assert!(!body.contains("data-filter-input"));
    assert!(!body.contains(SEARCH_SCRIPT));

    create_service(&app, "Service 9").await;

    let body = page(&app, "/events/new").await;
    assert!(body.contains("data-filter-input"));
    assert!(body.contains(SEARCH_SCRIPT));
}

#[tokio::test]
async fn services_come_after_loose_ones_and_follow_the_folder_order() {
    let app = spawn_with_publisher().await;
    let (zeta, zeta_services) = create_folder_with(&app, "Zeta Site", &["Zeta One"]).await;
    let (alpha, alpha_services) =
        create_folder_with(&app, "Alpha Site", &["Alpha One", "Alpha Two"]).await;
    let loose = create_service(&app, "Loose Service").await;
    FolderRepository::reorder(&app.pool, &[zeta, alpha])
        .await
        .expect("failed to order the folders");

    let body = page(&app, "/events/new").await;

    let expected = [
        at(&body, &service_marker(loose)),
        at(&body, &folder_marker(zeta)),
        at(&body, &service_marker(zeta_services[0])),
        at(&body, &folder_marker(alpha)),
        at(&body, &service_marker(alpha_services[0])),
        at(&body, &service_marker(alpha_services[1])),
    ];
    assert!(expected.is_sorted(), "{expected:?}");
    assert!(folder_section(&body, zeta).contains("<span class=\"count\">1</span>"));
    assert!(folder_section(&body, alpha).contains("<span class=\"count\">2</span>"));
}

#[tokio::test]
async fn an_empty_folder_is_left_out() {
    let app = spawn_with_publisher().await;
    let empty = create_folder(&app, "Empty Site").await;
    create_folder_with(&app, "Full Site", &["Full One"]).await;

    let body = page(&app, "/events/new").await;

    assert!(!body.contains("Empty Site"));
    assert!(!body.contains(&folder_marker(empty)));
    assert!(body.contains("Full Site"));
}

#[tokio::test]
async fn the_folder_box_is_not_sent_with_the_form() {
    let app = spawn_with_publisher().await;
    create_folder_with(&app, "Paris", &["Wi-Fi Paris"]).await;

    let body = page(&app, "/events/new").await;

    let tag = tag_around(&body, "data-folder-box");
    assert!(tag.starts_with("<input"), "{tag}");
    assert!(!tag.contains("name="), "{tag}");
}

#[tokio::test]
async fn editing_checks_a_folder_only_when_all_its_services_are_named() {
    let app = spawn_with_publisher().await;
    let (_, full_services) = create_folder_with(&app, "Full Site", &["Full One", "Full Two"]).await;
    let (_, part_services) = create_folder_with(&app, "Part Site", &["Part One", "Part Two"]).await;
    let all_of_full = create_incident_with(&app, &full_services).await;
    let one_of_part = create_incident_with(&app, &part_services[..1]).await;

    let body = page(&app, &format!("/events/{all_of_full}/edit")).await;
    let checked: Vec<bool> = folder_box_tags(&body)
        .iter()
        .map(|tag| tag.contains(" checked"))
        .collect();
    assert_eq!(checked, [true, false]);

    let body = page(&app, &format!("/events/{one_of_part}/edit")).await;
    let checked: Vec<bool> = folder_box_tags(&body)
        .iter()
        .map(|tag| tag.contains(" checked"))
        .collect();
    assert_eq!(checked, [false, false]);
}

#[tokio::test]
async fn creating_with_part_of_a_folder_links_only_those_services() {
    let app = spawn_with_publisher().await;
    let (_, services) = create_folder_with(&app, "Paris", &["Router", "Switch", "Firewall"]).await;

    let event = create_incident_with(&app, &services[..2]).await;

    let mut linked = EventRepository::service_ids(&app.pool, event)
        .await
        .expect("failed to read the linked services");
    linked.sort_unstable();
    assert_eq!(linked, services[..2]);
}

#[tokio::test]
async fn a_service_in_a_folder_is_searchable_by_the_folder_name() {
    let app = spawn_with_publisher().await;
    create_folder_with(&app, "Site de Paris", &["Wi-Fi Paris"]).await;

    let body = page(&app, "/events/new").await;

    assert!(body.contains("data-filter-text=\"Wi-Fi Paris Site de Paris\""));
}
