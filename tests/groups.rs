//! Groups of members: created from the Team page, then managed from their
//! own page (rename, add and remove members, delete), all of it for
//! administrators only.

mod common;

use reqwest::StatusCode;

use common::TestApp;
use statup::i18n::{I18n, LANGUAGES};
use statup::models::{Grants, Role, Visibility};
use statup::repositories::{FolderRepository, GroupRepository, ServiceRepository, UserRepository};

const ADMIN_EMAIL: &str = "admin@test.com";
const ADMIN_PASSWORD: &str = "admin_password_12";
const READER_PASSWORD: &str = "reader_password_12";

/// A page text, in the language a client that asks for none gets.
fn text(key: &str) -> String {
    I18n::new(LANGUAGES[0].code).t(key).to_string()
}

fn plural(key: &str, n: usize) -> String {
    I18n::new(LANGUAGES[0].code).plural(key, n)
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

async fn add_to_group(app: &TestApp, group: i64, user_ids: &[i64]) {
    GroupRepository::add_members(&app.pool, group, user_ids)
        .await
        .expect("failed to add members");
}

async fn group_names(app: &TestApp) -> Vec<String> {
    GroupRepository::list(&app.pool)
        .await
        .expect("failed to list groups")
        .into_iter()
        .map(|group| group.name)
        .collect()
}

async fn member_ids(app: &TestApp, group: i64) -> Vec<i64> {
    GroupRepository::members(&app.pool, group)
        .await
        .expect("failed to list members")
        .into_iter()
        .map(|member| member.id)
        .collect()
}

/// POST a form with a token taken from the Team page.
async fn post_team(
    app: &TestApp,
    path: &str,
    fields: &[(&str, &str)],
) -> (StatusCode, String, Option<String>) {
    let csrf = app.csrf_from("/admin/users").await;
    app.post_form(path, &csrf, fields).await
}

/// GET a page that must exist.
async fn page(app: &TestApp, path: &str) -> String {
    let (status, body) = app.get(path).await;
    assert_eq!(status, StatusCode::OK, "{path}");
    body
}

/// Follow a redirect and return the page it leads to.
async fn page_after(app: &TestApp, location: Option<String>) -> String {
    let location = location.expect("a redirect");
    let path = location.split('#').next().unwrap_or_default().to_string();
    page(app, &path).await
}

/// The part of a page between two markers.
fn between<'a>(body: &'a str, from: &str, to: &str) -> &'a str {
    let rest = &body[body
        .find(from)
        .unwrap_or_else(|| panic!("{from} not found"))..];
    &rest[..rest.find(to).unwrap_or_else(|| panic!("{to} not found"))]
}

/// What the text field with this id holds.
fn field_value(body: &str, id: &str) -> String {
    let tag = between(body, &format!(r#"id="{id}""#), ">");
    let value = &tag[tag.find(r#"value=""#).expect("field value not found") + r#"value=""#.len()..];
    between(value, "", "\"").to_string()
}

/// The members list of a group page, without the "Add members" panel.
fn member_rows(body: &str) -> &str {
    between(body, r#"<ul class="group-members">"#, "</ul>")
}

/// The checkboxes of the "Add members" panel.
fn picker(body: &str) -> &str {
    between(body, r#"class="picker-list""#, "data-filter-none")
}

/// The link to a group on the Team page.
fn group_link(body: &str, id: i64) -> &str {
    between(
        body,
        &format!(r#"<a class="group-link" href="/admin/groups/{id}""#),
        "</a>",
    )
}

fn assert_refused_name(body: &str, message_key: &str, typed: &str) {
    assert!(
        body.contains(r#"group-new" data-more-actions open>"#),
        "panel open"
    );
    assert!(body.contains(r#"id="new-group-error""#));
    assert!(body.contains(&text(message_key)), "{message_key}");
    assert_eq!(
        field_value(body, "new-group"),
        typed,
        "the typed name stays"
    );
}

#[tokio::test]
async fn an_admin_creates_a_group_and_lands_on_its_page() {
    let app = spawn_with_admin().await;

    let (status, _, location) =
        post_team(&app, "/admin/groups/new", &[("name", "  Compta  ")]).await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(group_names(&app).await, vec!["Compta".to_string()]);
    let id = GroupRepository::list(&app.pool).await.unwrap()[0].id;
    assert_eq!(
        location.as_deref(),
        Some(format!("/admin/groups/{id}?created=1").as_str())
    );
    let body = page_after(&app, location).await;
    assert!(body.contains(r#"<h1 class="page-title wrap-anywhere">Compta</h1>"#));
    assert!(body.contains(&text("admin.notice_group_created")));

    let team = page(&app, "/admin/users").await;
    let link = group_link(&team, id);
    assert!(link.contains("Compta"));
    assert!(link.contains(&plural("admin.group_members", 0)));
}

#[tokio::test]
async fn a_refused_group_name_creates_nothing() {
    let app = spawn_with_admin().await;
    create_group(&app, "Compta").await;
    create_group(&app, "Équipe").await;
    let too_long = "a".repeat(101);
    let refused = [
        ("compta", "validation.group_name_taken"),
        ("équipe", "validation.group_name_taken"),
        ("", "validation.group_name_required"),
        (too_long.as_str(), "validation.group_name_too_long"),
    ];

    for (typed, message_key) in refused {
        let (status, body, location) =
            post_team(&app, "/admin/groups/new", &[("name", typed)]).await;
        assert_eq!(status, StatusCode::OK, "{typed}");
        assert!(location.is_none());
        assert_refused_name(&body, message_key, typed);
    }

    let mut names = group_names(&app).await;
    names.sort();
    assert_eq!(names, vec!["Compta".to_string(), "Équipe".to_string()]);
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
        Some(format!("/admin/groups/{compta}?renamed=1").as_str())
    );
    let body = page_after(&app, location).await;
    assert!(body.contains(r#"<h1 class="page-title wrap-anywhere">Comptabilité</h1>"#));
    assert!(body.contains(&text("admin.notice_group_renamed")));
    assert!(!body.contains(r#"group-rename" data-more-actions open>"#));

    let (status, _, _) = post_team(&app, &path, &[("name", "COMPTABILITÉ")]).await;
    assert_eq!(
        status,
        StatusCode::SEE_OTHER,
        "its own name, in another case"
    );
    assert_eq!(group_names(&app).await, vec!["COMPTABILITÉ", "Paie"]);

    let (status, body, location) = post_team(&app, &path, &[("name", "paie")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(location.is_none());
    assert!(body.contains(r#"group-rename" data-more-actions open>"#));
    assert!(body.contains(r#"id="group-name-error""#));
    assert!(body.contains(&text("validation.group_name_taken")));
    assert_eq!(field_value(&body, "group-name"), "paie");
    assert_eq!(group_names(&app).await, vec!["COMPTABILITÉ", "Paie"]);

    let (status, _, _) = post_team(&app, "/admin/groups/9999/rename", &[("name", "Ghost")]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn members_are_added_to_a_group_from_its_page() {
    let app = spawn_with_admin().await;
    let ann = reader(&app, "ann@test.com", "Ann").await;
    let bob = reader(&app, "bob@test.com", "Bob").await;
    let cy = reader(&app, "cy@test.com", "Cy").await;
    let dee = reader(&app, "dee@test.com", "Dee").await;
    UserRepository::set_active(&app.pool, dee, false)
        .await
        .unwrap();
    let group = create_group(&app, "Compta").await;
    let group_path = format!("/admin/groups/{group}");

    let before = page(&app, &group_path).await;
    for offered in ["ann@test.com", "bob@test.com", "cy@test.com"] {
        assert!(picker(&before).contains(offered), "{offered}");
    }
    assert!(
        !picker(&before).contains("dee@test.com"),
        "a disabled member"
    );

    let (ann_id, bob_id) = (ann.to_string(), bob.to_string());
    let fields = [("user_ids", ann_id.as_str()), ("user_ids", bob_id.as_str())];
    let (status, _, location) = post_team(&app, &format!("{group_path}/members"), &fields).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some(format!("{group_path}?added=2").as_str())
    );
    let body = page_after(&app, location).await;
    assert!(body.contains(&plural("admin.notice_members_added", 2)));
    assert!(body.contains(r#"<span class="count">2</span>"#));
    for listed in ["Ann", "Bob"] {
        assert!(member_rows(&body).contains(listed), "{listed}");
    }
    assert_eq!(body.matches(">Retirer</button>").count(), 2);
    assert!(!picker(&body).contains("ann@test.com"));
    assert!(picker(&body).contains("cy@test.com"), "Cy can still join");

    let team = page(&app, "/admin/users").await;
    assert!(group_link(&team, group).contains(&plural("admin.group_members", 2)));
    assert_eq!(member_ids(&app, group).await, vec![ann, bob]);
    assert_ne!(cy, ann);
}

#[tokio::test]
async fn adding_a_member_twice_or_an_unknown_one_adds_nothing() {
    let app = spawn_with_admin().await;
    let ann = reader(&app, "ann@test.com", "Ann").await;
    let group = create_group(&app, "Compta").await;
    add_to_group(&app, group, &[ann]).await;
    let path = format!("/admin/groups/{group}/members");
    let ann_id = ann.to_string();

    let fields = [("user_ids", ann_id.as_str()), ("user_ids", "9999")];
    for sent in [&fields[..], &[]] {
        let (status, _, location) = post_team(&app, &path, sent).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            location.as_deref(),
            Some(format!("/admin/groups/{group}?added=0").as_str())
        );
        let body = page_after(&app, location).await;
        assert!(!body.contains("form-notice"), "nothing to say");
    }
    assert_eq!(member_ids(&app, group).await, vec![ann]);

    let (status, _, _) = post_team(&app, "/admin/groups/9999/members", &fields).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_member_is_removed_from_a_group_once() {
    let app = spawn_with_admin().await;
    let ann = reader(&app, "ann@test.com", "Ann").await;
    let bob = reader(&app, "bob@test.com", "Bob").await;
    let group = create_group(&app, "Compta").await;
    add_to_group(&app, group, &[ann, bob]).await;
    let path = format!("/admin/groups/{group}/members/{ann}/remove");

    let (status, _, location) = post_team(&app, &path, &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some(format!("/admin/groups/{group}?removed={ann}").as_str())
    );
    let body = page_after(&app, location).await;
    assert!(body.contains(&format!(
        "<q>Ann</q> {}",
        text("admin.notice_member_removed")
    )));
    assert!(!member_rows(&body).contains("Ann"));
    assert!(member_rows(&body).contains("Bob"));
    assert!(picker(&body).contains("ann@test.com"), "Ann can join again");
    assert_eq!(member_ids(&app, group).await, vec![bob]);

    let (status, _, _) = post_team(&app, &path, &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no longer a member");
}

#[tokio::test]
async fn deleting_a_group_names_what_it_closes_and_clears_its_traces() {
    let app = spawn_with_admin().await;
    let ann = reader(&app, "ann@test.com", "Ann").await;
    let paie = create_group(&app, "Paie").await;
    let compta = create_group(&app, "Compta").await;
    add_to_group(&app, paie, &[ann]).await;
    add_to_group(&app, compta, &[ann]).await;
    let only_paie =
        ServiceRepository::create(&app.pool, "Coffre Zephyr", "coffre", None, None, None)
            .await
            .unwrap();
    ServiceRepository::set_visibility(
        &app.pool,
        only_paie.id,
        Visibility::Groups,
        Grants::groups(&[paie]),
    )
    .await
    .unwrap();
    let shared = ServiceRepository::create(&app.pool, "Annuaire K2", "annuaire", None, None, None)
        .await
        .unwrap();
    ServiceRepository::set_visibility(
        &app.pool,
        shared.id,
        Visibility::Groups,
        Grants::groups(&[paie, compta]),
    )
    .await
    .unwrap();

    let body = page(&app, &format!("/admin/groups/{paie}")).await;
    assert!(body.contains(&text("admin.delete_group_members")));
    let one_closes = I18n::new(LANGUAGES[0].code).plural("admin.delete_group_closes", 1);
    assert!(
        body.contains(&one_closes),
        "it counts the service only it opens, not the one another group also opens"
    );
    assert!(
        !between(&body, "danger-zone", "</section>").contains("Coffre Zephyr"),
        "a count, not a list of names"
    );
    let body = page(&app, &format!("/admin/groups/{compta}")).await;
    assert!(!body.contains(&one_closes), "nothing closes");

    let path = format!("/admin/groups/{paie}/delete");
    let (status, _, location) = post_team(&app, &path, &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some("/admin/users?group_deleted=1#groups")
    );
    let team = page_after(&app, location).await;
    assert!(team.contains(&text("admin.notice_group_deleted")));

    assert_eq!(group_names(&app).await, vec!["Compta".to_string()]);
    assert_eq!(member_ids(&app, compta).await, vec![ann]);
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

    let (status, _, _) = post_team(&app, &path, &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "already gone");
    let (status, _, _) = post_team(&app, "/admin/groups/9999/delete", &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app.get(&format!("/admin/groups/{paie}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_publisher_cannot_see_or_touch_groups() {
    let app = TestApp::spawn().await;
    app.create_user(
        "pub@test.com",
        "publisher_pass_12",
        "Publisher",
        Role::Publisher,
    )
    .await;
    let ann = reader(&app, "ann@test.com", "Ann").await;
    let bob = reader(&app, "bob@test.com", "Bob").await;
    let compta = create_group(&app, "Compta").await;
    add_to_group(&app, compta, &[ann]).await;
    app.login("pub@test.com", "publisher_pass_12").await;
    let csrf = app.csrf_from("/").await;
    let (ann_id, bob_id) = (ann.to_string(), bob.to_string());
    let group = format!("/admin/groups/{compta}");

    let (status, _) = app.get(&group).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "the group page");

    let attempts: [(String, Vec<(&str, &str)>); 5] = [
        ("/admin/groups/new".to_string(), vec![("name", "Paie")]),
        (format!("{group}/rename"), vec![("name", "Renamed")]),
        (format!("{group}/members"), vec![("user_ids", &bob_id)]),
        (format!("{group}/members/{ann_id}/remove"), vec![]),
        (format!("{group}/delete"), vec![]),
    ];
    for (path, fields) in &attempts {
        let (status, _, _) = app.post_form(path, &csrf, fields).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }

    assert_eq!(group_names(&app).await, vec!["Compta".to_string()]);
    assert_eq!(member_ids(&app, compta).await, vec![ann]);
}

#[tokio::test]
async fn only_readers_are_offered_and_added_to_a_group() {
    let app = spawn_with_admin().await;
    let ann = reader(&app, "ann@test.com", "Ann").await;
    let editor = app
        .create_user("ed@test.com", READER_PASSWORD, "Ed", Role::Publisher)
        .await;
    let group = create_group(&app, "Compta").await;
    let group_path = format!("/admin/groups/{group}");

    let body = page(&app, &group_path).await;
    assert!(picker(&body).contains("ann@test.com"));
    assert!(
        !picker(&body).contains("ed@test.com"),
        "an editor sees every service"
    );
    assert!(
        !picker(&body).contains(ADMIN_EMAIL),
        "so does an administrator"
    );

    let (ann_id, editor_id) = (ann.to_string(), editor.to_string());
    let fields = [
        ("user_ids", ann_id.as_str()),
        ("user_ids", editor_id.as_str()),
    ];
    post_team(&app, &format!("{group_path}/members"), &fields).await;

    assert_eq!(member_ids(&app, group).await, vec![ann]);
}

#[tokio::test]
async fn a_group_page_lists_the_restricted_services_it_opens() {
    let app = spawn_with_admin().await;
    let compta = create_group(&app, "Compta").await;
    let lyon = create_group(&app, "Lyon").await;
    let mut ids = Vec::new();
    for name in [
        "Paie Alpha",
        "Notes Beta",
        "Wiki Gamma",
        "Imprimantes Delta",
    ] {
        let service =
            ServiceRepository::create(&app.pool, name, &name.to_lowercase(), None, None, None)
                .await
                .unwrap();
        ids.push(service.id);
    }
    ServiceRepository::set_visibility(
        &app.pool,
        ids[0],
        Visibility::Groups,
        Grants::groups(&[compta]),
    )
    .await
    .unwrap();
    ServiceRepository::set_visibility(
        &app.pool,
        ids[3],
        Visibility::Groups,
        Grants::groups(&[lyon]),
    )
    .await
    .unwrap();
    let folder = FolderRepository::create(&app.pool, "Finances")
        .await
        .unwrap();
    FolderRepository::set_visibility(
        &app.pool,
        folder,
        Visibility::Groups,
        Grants::groups(&[compta]),
    )
    .await
    .unwrap();
    FolderRepository::add_services(&app.pool, folder, &[ids[1]])
        .await
        .unwrap();

    let body = page(&app, &format!("/admin/groups/{compta}")).await;
    let card = between(&body, "group-services", "</section>");
    assert!(card.contains("Paie Alpha"), "opened to the group itself");
    assert!(card.contains("Notes Beta"), "opened through its folder");
    let folder_label =
        I18n::new(LANGUAGES[0].code).tf("admin.group_service_folder", &[("folder", "Finances")]);
    assert!(card.contains(&folder_label));
    assert!(
        !card.contains("Wiki Gamma"),
        "a public service is not the group's"
    );
    assert!(
        !card.contains("Imprimantes Delta"),
        "another group's service"
    );

    let body = page(&app, &format!("/admin/groups/{lyon}")).await;
    assert!(between(&body, "group-services", "</section>").contains("Imprimantes Delta"));
    let empty = create_group(&app, "Vide").await;
    let body = page(&app, &format!("/admin/groups/{empty}")).await;
    assert!(
        between(&body, "group-services", "</section>")
            .contains(&text("admin.group_services_empty"))
    );
}
