//! Groups of members on the Team page: creating, renaming and deleting a
//! group, choosing the groups of a member, and keeping all of it to
//! administrators.

mod common;

use reqwest::StatusCode;

use common::TestApp;
use statup::i18n::{I18n, LANGUAGES};
use statup::models::{Role, Visibility};
use statup::repositories::{GroupRepository, ServiceRepository, UserRepository};

const ADMIN_EMAIL: &str = "admin@test.com";
const ADMIN_PASSWORD: &str = "admin_password_12";
const READER_PASSWORD: &str = "reader_password_12";

/// A page text, in the language a client that asks for none gets.
fn text(key: &str) -> String {
    I18n::new(LANGUAGES[0].code).t(key).to_string()
}

fn member_count(n: usize) -> String {
    I18n::new(LANGUAGES[0].code).plural("admin.group_members", n)
}

/// An instance with a signed-in administrator.
async fn spawn_with_admin() -> TestApp {
    let app = TestApp::spawn().await;
    app.create_user(ADMIN_EMAIL, ADMIN_PASSWORD, "Admin", Role::Admin)
        .await;
    app.login(ADMIN_EMAIL, ADMIN_PASSWORD).await;
    app
}

async fn reader(app: &TestApp, email: &str, name: &str) -> i64 {
    app.create_user(email, READER_PASSWORD, name, Role::Reader)
        .await
}

async fn create_group(app: &TestApp, name: &str) -> i64 {
    GroupRepository::create(&app.pool, name)
        .await
        .expect("failed to create group")
}

async fn group_names(app: &TestApp) -> Vec<String> {
    GroupRepository::list(&app.pool)
        .await
        .expect("failed to list groups")
        .into_iter()
        .map(|group| group.name)
        .collect()
}

async fn post_team(
    app: &TestApp,
    path: &str,
    fields: &[(&str, &str)],
) -> (StatusCode, String, Option<String>) {
    let csrf = app.csrf_from("/admin/users").await;
    app.post_form(path, &csrf, fields).await
}

/// Follow a redirect to the Team page and return its body.
async fn page_after(app: &TestApp, location: Option<String>) -> String {
    let location = location.expect("a redirect to the team page");
    let path = location.split('#').next().unwrap_or_default().to_string();
    let (status, body) = app.get(&path).await;
    assert_eq!(status, StatusCode::OK);
    body
}

/// The grey line of groups under a member, without the edit panel.
fn groups_line(body: &str, member_id: i64) -> String {
    let row = &body[body
        .find(&format!(r#"id="member-{member_id}""#))
        .expect("member row not found")..];
    let start = row
        .find(r#"member-groups">"#)
        .expect("groups line not found");
    let line = &row[start..];
    let end = line.find("&nbsp;").expect("end of groups line not found");
    line[..end].to_string()
}

/// What the "new group" field holds.
fn new_group_field(body: &str) -> String {
    let input = &body[body.find(r#"id="new-group""#).expect("field not found")..];
    let input = &input[..input.find('>').expect("end of field not found")];
    let start = input.find(r#"value=""#).expect("field value not found") + r#"value=""#.len();
    let len = input[start..].find('"').expect("end of value not found");
    input[start..start + len].to_string()
}

/// Whether the group's box is ticked in a form of the page, from `from`.
fn box_ticked(body: &str, from: &str, group_id: i64) -> bool {
    let after = &body[body.find(from).expect("form not found")..];
    after.contains(&format!(
        r#"name="group_ids" value="{group_id}" class="checkbox" checked"#
    ))
}

#[tokio::test]
async fn an_admin_creates_a_group() {
    let app = spawn_with_admin().await;

    let (status, _, location) =
        post_team(&app, "/admin/groups/new", &[("name", "  Compta  ")]).await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(group_names(&app).await, vec!["Compta".to_string()]);
    let id = GroupRepository::list(&app.pool).await.unwrap()[0].id;
    assert_eq!(
        location.as_deref(),
        Some(format!("/admin/users?group_created={id}#groups").as_str())
    );
    let body = page_after(&app, location).await;
    assert!(body.contains(&format!(
        "{} <q>Compta</q>",
        text("admin.notice_group_created")
    )));
    assert!(body.contains(&format!(r#"admin-row-sub">{}</p>"#, member_count(0))));
}

#[tokio::test]
async fn a_refused_group_name_creates_nothing() {
    let app = spawn_with_admin().await;
    create_group(&app, "Compta").await;

    let (status, body, location) =
        post_team(&app, "/admin/groups/new", &[("name", "compta")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(location.is_none());
    assert!(body.contains(&text("validation.group_name_taken")));
    assert_eq!(new_group_field(&body), "compta", "the typed name stays");

    let (status, body, _) = post_team(&app, "/admin/groups/new", &[("name", "   ")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(&text("validation.group_name_required")));

    let too_long = "a".repeat(101);
    let (status, body, _) = post_team(&app, "/admin/groups/new", &[("name", &too_long)]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(&text("validation.group_name_too_long")));
    assert_eq!(new_group_field(&body), too_long);

    assert_eq!(group_names(&app).await, vec!["Compta".to_string()]);
}

#[tokio::test]
async fn a_group_is_renamed_unless_the_name_is_another_groups() {
    let app = spawn_with_admin().await;
    let compta = create_group(&app, "Compta").await;
    create_group(&app, "Paie").await;
    let path = format!("/admin/groups/{compta}/rename");

    let (status, _, location) = post_team(&app, &path, &[("name", "Comptabilité")]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some(format!("/admin/users?group_renamed={compta}#groups").as_str())
    );
    let body = page_after(&app, location).await;
    assert!(body.contains(&format!(
        "{} <q>Comptabilité</q>",
        text("admin.notice_group_renamed")
    )));

    let (status, _, _) = post_team(&app, &path, &[("name", "COMPTABILITÉ")]).await;
    assert_eq!(
        status,
        StatusCode::SEE_OTHER,
        "its own name, in another case"
    );
    let (status, _, _) = post_team(&app, &path, &[("name", "Comptabilité")]).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "its own name, unchanged");

    let (status, body, location) = post_team(&app, &path, &[("name", "paie")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(location.is_none());
    assert!(body.contains(&text("validation.group_name_taken")));

    let (status, _, _) = post_team(&app, "/admin/groups/9999/rename", &[("name", "Ghost")]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let mut names = group_names(&app).await;
    names.sort();
    assert_eq!(names, vec!["Comptabilité".to_string(), "Paie".to_string()]);
}

#[tokio::test]
async fn an_admin_sets_the_groups_of_a_member() {
    let app = spawn_with_admin().await;
    let member = reader(&app, "reader@test.com", "Reader").await;
    let compta = create_group(&app, "Compta").await;
    let paie = create_group(&app, "Paie").await;
    create_group(&app, "Achats").await;
    let path = format!("/admin/users/{member}/groups");
    let (compta_field, paie_field) = (compta.to_string(), paie.to_string());

    let (status, _, location) = post_team(
        &app,
        &path,
        &[("group_ids", &compta_field), ("group_ids", &paie_field)],
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some(format!("/admin/users?groups_of={member}#member-{member}").as_str())
    );
    let body = page_after(&app, location).await;
    assert!(body.contains(&format!(
        "{} <q>Reader</q>",
        text("admin.notice_member_groups")
    )));
    assert!(groups_line(&body, member).contains("Compta, Paie"));
    assert!(body.contains(&format!(r#"admin-row-sub">{}</p>"#, member_count(1))));

    let (status, _, _) = post_team(&app, &path, &[("group_ids", &paie_field)]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        GroupRepository::ids_of_member(&app.pool, member)
            .await
            .unwrap(),
        vec![paie]
    );

    let (status, _, _) = post_team(&app, &path, &[("group_ids", "9999")]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(
        GroupRepository::ids_of_member(&app.pool, member)
            .await
            .unwrap()
            .is_empty(),
        "an unknown group is dropped, and the others were replaced"
    );

    let (status, _, _) = post_team(&app, &path, &[("group_ids", &compta_field)]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, _, location) = post_team(&app, &path, &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(
        GroupRepository::ids_of_member(&app.pool, member)
            .await
            .unwrap()
            .is_empty()
    );
    let body = page_after(&app, location).await;
    assert!(groups_line(&body, member).contains(&text("admin.no_group")));

    let (status, _, _) = post_team(
        &app,
        "/admin/users/9999/groups",
        &[("group_ids", &compta_field)],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_member_is_added_to_the_ticked_groups() {
    let app = spawn_with_admin().await;
    let compta = create_group(&app, "Compta").await;
    let paie = create_group(&app, "Paie").await;
    let achats = create_group(&app, "Achats").await;
    let (compta_field, achats_field) = (compta.to_string(), achats.to_string());

    let (status, _, _) = post_team(
        &app,
        "/admin/users/new",
        &[
            ("display_name", "Newcomer"),
            ("email", "new@test.com"),
            ("role", "reader"),
            ("group_ids", &compta_field),
            ("group_ids", &achats_field),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let member = UserRepository::find_by_email(&app.pool, "new@test.com")
        .await
        .unwrap()
        .expect("member not created");
    let mut ids = GroupRepository::ids_of_member(&app.pool, member.id)
        .await
        .unwrap();
    ids.sort_unstable();
    assert_eq!(ids, vec![compta, achats]);
    assert_ne!(paie, compta);

    let (status, body, _) = post_team(
        &app,
        "/admin/users/new",
        &[
            ("display_name", "Refused"),
            ("email", "not-an-email"),
            ("role", "reader"),
            ("group_ids", &compta_field),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("form-error"));
    let form = r#"action="/admin/users/new""#;
    assert!(
        box_ticked(&body, form, compta),
        "a refused form keeps its boxes"
    );
    assert!(!box_ticked(&body, form, paie));
}

#[tokio::test]
async fn deleting_a_group_names_what_it_closes_and_clears_its_traces() {
    let app = spawn_with_admin().await;
    let member = reader(&app, "reader@test.com", "Reader").await;
    let paie = create_group(&app, "Paie").await;
    let compta = create_group(&app, "Compta").await;
    GroupRepository::set_member_groups(&app.pool, member, &[paie, compta])
        .await
        .unwrap();
    let only_paie =
        ServiceRepository::create(&app.pool, "Coffre Zephyr", "coffre", None, None, None)
            .await
            .unwrap();
    ServiceRepository::set_visibility(&app.pool, only_paie.id, Visibility::Groups, &[paie])
        .await
        .unwrap();
    let shared = ServiceRepository::create(&app.pool, "Annuaire K2", "annuaire", None, None, None)
        .await
        .unwrap();
    ServiceRepository::set_visibility(&app.pool, shared.id, Visibility::Groups, &[paie, compta])
        .await
        .unwrap();

    let (_, page) = app.get("/admin/users").await;
    assert!(page.contains(&format!(
        "{} Coffre Zephyr.",
        text("admin.delete_group_closes")
    )));
    assert!(
        !page.contains("Annuaire K2"),
        "a service another group still opens is not named"
    );

    let (status, _, location) = post_team(&app, &format!("/admin/groups/{paie}/delete"), &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some("/admin/users?group_deleted=1#groups")
    );
    let body = page_after(&app, location).await;
    assert!(body.contains(&text("admin.notice_group_deleted")));

    assert_eq!(group_names(&app).await, vec!["Compta".to_string()]);
    assert_eq!(
        GroupRepository::ids_of_member(&app.pool, member)
            .await
            .unwrap(),
        vec![compta]
    );
    let access: Vec<(i64, i64)> = sqlx::query_as("SELECT service_id, group_id FROM service_access")
        .fetch_all(&app.pool)
        .await
        .unwrap();
    assert_eq!(access, vec![(shared.id, compta)]);
    let memberships: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id = ?")
            .bind(paie)
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(memberships, 0);

    let (status, _, _) = post_team(&app, &format!("/admin/groups/{paie}/delete"), &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "already gone");
    let (status, _, _) = post_team(&app, "/admin/groups/9999/delete", &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_publisher_cannot_touch_groups() {
    let app = TestApp::spawn().await;
    app.create_user(
        "pub@test.com",
        "publisher_pass_12",
        "Publisher",
        Role::Publisher,
    )
    .await;
    let member = reader(&app, "reader@test.com", "Reader").await;
    let compta = create_group(&app, "Compta").await;
    app.login("pub@test.com", "publisher_pass_12").await;
    let csrf = app.csrf_from("/").await;
    let compta_field = compta.to_string();

    let attempts: [(String, Vec<(&str, &str)>); 4] = [
        ("/admin/groups/new".to_string(), vec![("name", "Paie")]),
        (
            format!("/admin/groups/{compta}/rename"),
            vec![("name", "Renamed")],
        ),
        (format!("/admin/groups/{compta}/delete"), vec![]),
        (
            format!("/admin/users/{member}/groups"),
            vec![("group_ids", &compta_field)],
        ),
    ];
    for (path, fields) in &attempts {
        let (status, _, _) = app.post_form(path, &csrf, fields).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }

    assert_eq!(group_names(&app).await, vec!["Compta".to_string()]);
    assert!(
        GroupRepository::ids_of_member(&app.pool, member)
            .await
            .unwrap()
            .is_empty()
    );
}
