//! Integration tests for the event lifecycle.
//!
//! Validates the full HTTP flow: creation, transitions, updates, closure.
//! Also exercises service status recalculation.

mod common;

use chrono::Utc;
use reqwest::StatusCode;

use common::{TestApp, extract_csrf_token};
use statup::i18n::{I18n, LANGUAGES};
use statup::models::{CheckKind, CheckedService, Lifecycle, Role, ServiceCheck, ServiceStatus};
use statup::repositories::{EventRepository, OutageCause, ServiceRepository};
use statup::services::{Finding, MonitorState, Prober, Report, ServiceService, run_round};

/// A page text with its `{minutes}` filled, in the language a client that asks for none gets.
fn text_with_minutes(key: &str, minutes: &str) -> String {
    I18n::new(LANGUAGES[0].code).tf(key, &[("minutes", minutes)])
}

impl TestApp {
    /// POST via the `X-CSRF-Token` header, the way htmx sends it.
    async fn post_form_with_header_csrf(
        &self,
        path: &str,
        fields: &[(&str, &str)],
    ) -> (StatusCode, String, Option<String>) {
        let csrf = self.csrf_from("/").await;

        let resp = self
            .client
            .post(self.url(path))
            .header("x-csrf-token", &csrf)
            .form(fields)
            .send()
            .await
            .expect("POST request failed");

        let status = resp.status();
        let location = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned);
        let body = resp.text().await.unwrap_or_default();
        (status, body, location)
    }

    async fn setup_publisher(&self) {
        self.create_user(
            "publisher@example.com",
            "publisher_pass_12",
            "Publisher",
            Role::Publisher,
        )
        .await;
        self.login("publisher@example.com", "publisher_pass_12")
            .await;
    }

    async fn create_service(&self, name: &str) -> i64 {
        let slug = name.to_lowercase().replace(' ', "-");
        let service = ServiceRepository::create(&self.pool, name, &slug, None, None, None)
            .await
            .expect("failed to create service");
        service.id
    }

    async fn set_state_by_hand(&self, service_id: i64, status: &str) {
        let (code, _, _) = self
            .post_form_with_header_csrf(
                &format!("/services/{service_id}/status"),
                &[("status", status)],
            )
            .await;
        assert_eq!(code, StatusCode::SEE_OTHER);
    }

    async fn service(&self, id: i64) -> statup::models::Service {
        ServiceRepository::find_by_id(&self.pool, id)
            .await
            .expect("db error")
            .expect("service not found")
    }

    async fn submit_create_event(
        &self,
        base_fields: Vec<(&str, String)>,
        service_ids: &[i64],
    ) -> String {
        let csrf = self.csrf_from("/events/new").await;

        let mut fields = base_fields;
        for id in service_ids {
            fields.push(("service_ids", id.to_string()));
        }
        let fields_ref: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();

        let (status, _body, location) = self.post_form("/events/new", &csrf, &fields_ref).await;
        assert_eq!(
            status,
            StatusCode::SEE_OTHER,
            "event creation should redirect"
        );
        location.expect("should have Location header")
    }

    async fn create_incident(
        &self,
        title: &str,
        description: &str,
        severity: &str,
        service_ids: &[i64],
    ) -> String {
        self.submit_create_event(
            vec![
                ("title", title.to_string()),
                ("description", description.to_string()),
                ("kind", "incident".to_string()),
                ("severity", severity.to_string()),
            ],
            service_ids,
        )
        .await
    }

    async fn create_planned_maintenance(
        &self,
        title: &str,
        description: &str,
        severity: &str,
        service_ids: &[i64],
    ) -> String {
        self.submit_create_event(
            vec![
                ("title", title.to_string()),
                ("description", description.to_string()),
                ("kind", "maintenance".to_string()),
                ("severity", severity.to_string()),
                ("planned", "on".to_string()),
                ("planned_start", in_two_days()),
            ],
            service_ids,
        )
        .await
    }

    async fn create_publication(
        &self,
        title: &str,
        description: &str,
        category: &str,
        service_ids: &[i64],
    ) -> String {
        self.submit_create_event(
            vec![
                ("title", title.to_string()),
                ("description", description.to_string()),
                ("kind", "publication".to_string()),
                ("category", category.to_string()),
            ],
            service_ids,
        )
        .await
    }
}

/// A start the maintenance schedule will not reach during the test, in the
/// form a `datetime-local` field sends.
fn in_two_days() -> String {
    (statup::clock::local(&chrono::Utc::now()) + chrono::Duration::days(2))
        .format("%Y-%m-%dT%H:%M")
        .to_string()
}

fn event_id_from_path(path: &str) -> i64 {
    path.split('?')
        .next()
        .and_then(|p| p.rsplit('/').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("could not parse event ID from path: {path}"))
}

#[tokio::test]
async fn create_incident_and_verify_detail() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;

    let path = app
        .create_incident(
            "Database outage",
            "The primary database is down",
            "critical",
            &[],
        )
        .await;

    let (status, body) = app.get(&path).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Database outage"), "should show event title");
    assert!(
        body.contains("En analyse") || body.contains("Investigating"),
        "incident should start in Investigating lifecycle"
    );
}

#[tokio::test]
async fn full_incident_lifecycle_with_service_status() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;

    let service_id = app.create_service("API Gateway").await;

    let svc = ServiceRepository::find_by_id(&app.pool, service_id)
        .await
        .expect("db error")
        .expect("service not found");
    assert_eq!(svc.status, ServiceStatus::Operational);

    let path = app
        .create_incident(
            "API Gateway down",
            "The API gateway is not responding",
            "critical",
            &[service_id],
        )
        .await;
    let event_id = event_id_from_path(&path);

    let svc = ServiceRepository::find_by_id(&app.pool, service_id)
        .await
        .expect("db error")
        .expect("service not found");
    assert_eq!(
        svc.status,
        ServiceStatus::MajorOutage,
        "critical incident should cause MajorOutage"
    );

    let (status, _, _) = app
        .post_form_with_header_csrf(
            &format!("/events/{event_id}/updates"),
            &[("lifecycle", "in_progress")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let (_, detail_body) = app.get(&path).await;
    let csrf = extract_csrf_token(&detail_body);
    let (status, _, _) = app
        .post_form(
            &format!("/events/{event_id}/updates"),
            &csrf,
            &[("message", "Root cause identified: disk full")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let (_, body) = app.get(&path).await;
    assert!(
        body.contains("Root cause identified") || body.contains("disk full"),
        "update should appear on event detail"
    );

    let (status, _, _) = app
        .post_form_with_header_csrf(
            &format!("/events/{event_id}/updates"),
            &[("lifecycle", "monitoring")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let (status, _, _) = app
        .post_form_with_header_csrf(
            &format!("/events/{event_id}/updates"),
            &[("lifecycle", "resolved"), ("message", "Problème résolu")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let (_, body) = app.get(&path).await;
    assert!(
        body.contains("Resolved") || body.contains("resolved") || body.contains("Résolu"),
        "event should be resolved"
    );

    let svc = ServiceRepository::find_by_id(&app.pool, service_id)
        .await
        .expect("db error")
        .expect("service not found");
    assert_eq!(
        svc.status,
        ServiceStatus::Operational,
        "service should return to operational after incident resolved"
    );
}

#[tokio::test]
async fn scheduled_maintenance_lifecycle() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;

    let service_id = app.create_service("Auth Service").await;

    let path = app
        .create_planned_maintenance(
            "Planned DB migration",
            "Migrating to new schema",
            "minor",
            &[service_id],
        )
        .await;
    let event_id = event_id_from_path(&path);

    let svc = ServiceRepository::find_by_id(&app.pool, service_id)
        .await
        .expect("db error")
        .expect("service not found");
    assert_eq!(
        svc.status,
        ServiceStatus::Operational,
        "a maintenance still to come leaves the service as it is"
    );

    let (_, body) = app.get(&path).await;
    assert!(
        body.contains("Programmée") || body.contains("Scheduled"),
        "planned maintenance should start in Scheduled lifecycle"
    );

    let (status, _, _) = app
        .post_form_with_header_csrf(
            &format!("/events/{event_id}/updates"),
            &[("lifecycle", "in_progress")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let svc = ServiceRepository::find_by_id(&app.pool, service_id)
        .await
        .expect("db error")
        .expect("service not found");
    assert_eq!(
        svc.status,
        ServiceStatus::Maintenance,
        "work under way puts the service in maintenance"
    );

    let (status, _, _) = app
        .post_form_with_header_csrf(
            &format!("/events/{event_id}/updates"),
            &[
                ("lifecycle", "completed"),
                ("message", "Maintenance terminée"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let svc = ServiceRepository::find_by_id(&app.pool, service_id)
        .await
        .expect("db error")
        .expect("service not found");
    assert_eq!(svc.status, ServiceStatus::Operational);
}

#[tokio::test]
async fn invalid_lifecycle_transition_is_rejected() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;

    let path = app
        .create_incident(
            "Test transition",
            "Testing invalid transitions",
            "minor",
            &[],
        )
        .await;
    let event_id = event_id_from_path(&path);

    let (status, body, _) = app
        .post_form_with_header_csrf(
            &format!("/events/{event_id}/updates"),
            &[("lifecycle", "scheduled")],
        )
        .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "the page comes back with the refusal"
    );
    assert!(
        body.contains("pas possible"),
        "invalid transition should be refused in the page"
    );
}

#[tokio::test]
async fn reader_cannot_create_events() {
    let app = TestApp::spawn().await;

    app.create_user(
        "reader@example.com",
        "reader_pass_1234",
        "Reader",
        Role::Reader,
    )
    .await;
    app.login("reader@example.com", "reader_pass_1234").await;

    let (status, _) = app.get("/events/new").await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "reader should not access /events/new"
    );
}

#[tokio::test]
async fn multiple_events_worst_status_wins() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;

    let service_id = app.create_service("Payment Service").await;

    let minor_path = app
        .create_incident(
            "Slow payments",
            "Payment processing is slow",
            "minor",
            &[service_id],
        )
        .await;

    let svc = ServiceRepository::find_by_id(&app.pool, service_id)
        .await
        .expect("db error")
        .expect("service not found");
    assert_eq!(svc.status, ServiceStatus::Degraded);

    app.create_incident(
        "Payment gateway down",
        "Gateway unreachable",
        "critical",
        &[service_id],
    )
    .await;

    let svc = ServiceRepository::find_by_id(&app.pool, service_id)
        .await
        .expect("db error")
        .expect("service not found");
    assert_eq!(
        svc.status,
        ServiceStatus::MajorOutage,
        "worst status should win"
    );

    let minor_id = event_id_from_path(&minor_path);
    let (status, _, _) = app
        .post_form_with_header_csrf(
            &format!("/events/{minor_id}/updates"),
            &[("lifecycle", "resolved"), ("message", "Problème résolu")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let svc = ServiceRepository::find_by_id(&app.pool, service_id)
        .await
        .expect("db error")
        .expect("service not found");
    assert_eq!(
        svc.status,
        ServiceStatus::MajorOutage,
        "should stay MajorOutage while critical incident is active"
    );
}

#[tokio::test]
async fn publication_does_not_affect_service_status() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;

    let service_id = app.create_service("Publication Service").await;

    app.create_publication(
        "New feature shipped",
        "We shipped dark mode",
        "changelog",
        &[service_id],
    )
    .await;

    let svc = ServiceRepository::find_by_id(&app.pool, service_id)
        .await
        .expect("db error")
        .expect("service not found");
    assert_eq!(
        svc.status,
        ServiceStatus::Operational,
        "publication should not affect service status"
    );
}

/// An incident its author already works on opens at the step they chose;
/// one already under watch leaves its service as it is.
#[tokio::test]
async fn an_incident_opens_at_the_chosen_step() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;

    for (step, expected, service_status) in [
        (
            "in_progress",
            Lifecycle::InProgress,
            ServiceStatus::MajorOutage,
        ),
        (
            "monitoring",
            Lifecycle::Monitoring,
            ServiceStatus::Operational,
        ),
    ] {
        let service_id = app.create_service(&format!("Service {step}")).await;
        let location = app
            .submit_create_event(
                vec![
                    ("title", format!("Declared at {step}")),
                    ("kind", "incident".to_string()),
                    ("severity", "critical".to_string()),
                    ("opening_step", step.to_string()),
                ],
                &[service_id],
            )
            .await;
        let id: i64 = location
            .trim_start_matches("/events/")
            .split('?')
            .next()
            .and_then(|id| id.parse().ok())
            .expect("event id in the location");
        let event = EventRepository::find_by_id(&app.pool, id)
            .await
            .expect("db error")
            .expect("event not found");
        assert_eq!(event.lifecycle, Some(expected), "opening step {step}");
        assert_eq!(app.service(service_id).await.status, service_status);
    }
}

/// Maintenance without downtime leaves its services up while it runs;
/// ordinary maintenance puts them under maintenance.
#[tokio::test]
async fn maintenance_without_downtime_keeps_its_services_up() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;

    for (keeps_up, expected) in [
        (true, ServiceStatus::Operational),
        (false, ServiceStatus::Maintenance),
    ] {
        let service_id = app.create_service(&format!("Service {keeps_up}")).await;
        let mut fields = vec![
            ("title", format!("Work, keeps up: {keeps_up}")),
            ("kind", "maintenance".to_string()),
        ];
        if keeps_up {
            fields.push(("keeps_services_up", "on".to_string()));
        }
        app.submit_create_event(fields, &[service_id]).await;
        assert_eq!(
            app.service(service_id).await.status,
            expected,
            "keeps up: {keeps_up}"
        );
    }
}

/// The services page names the event that holds a service in its state,
/// in place of the control for the state set by hand.
#[tokio::test]
async fn services_page_names_the_event_holding_a_service() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Network").await;
    let path = app
        .submit_create_event(
            vec![
                ("title", "Router swap".to_string()),
                ("kind", "maintenance".to_string()),
            ],
            &[service_id],
        )
        .await;
    let event_path = path.split('?').next().unwrap_or_default().to_string();

    let (_, page) = app.get("/services").await;
    assert!(
        page.contains(&format!(r#"href="{event_path}">Router swap</a>"#)),
        "the event is named and linked: {page}"
    );
    assert!(
        !page.contains(&format!(r#"id="status-{service_id}""#)),
        "the control waits for the end of the event"
    );
}

/// A refused creation re-renders the form with everything the author typed.
#[tokio::test]
async fn rejected_event_creation_gives_the_input_back() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Payment gateway").await;

    let (_, form) = app.get("/events/new").await;
    let csrf = extract_csrf_token(&form);
    let service_id_str = service_id.to_string();

    let (status, body, _) = app
        .post_form(
            "/events/new",
            &csrf,
            &[
                ("title", "   "),
                ("description", "Checkout has been failing since 02:14 UTC."),
                ("kind", "incident"),
                ("severity", "critical"),
                ("planned_start", "2026-09-07T02:14"),
                ("service_ids", &service_id_str),
            ],
        )
        .await;

    assert_eq!(status, StatusCode::OK, "should re-render the form");
    assert!(
        body.contains("Checkout has been failing since 02:14 UTC."),
        "description should survive the rejection"
    );
    assert!(
        body.contains("2026-09-07T02:14"),
        "planned start should survive the rejection"
    );
    let critical = body
        .find(r#"value="critical""#)
        .expect("severity choice rendered");
    let tag_end = critical + body[critical..].find('>').expect("tag closes");
    assert!(
        body[critical..tag_end].contains("checked"),
        "severity should stay checked"
    );
    assert!(
        body.contains(r#"action="/events/new""#),
        "the form should still post to the creation route"
    );
}

/// The same rule on the service form.
#[tokio::test]
async fn rejected_service_creation_gives_the_input_back() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;

    let (_, form) = app.get("/services/new").await;
    let csrf = extract_csrf_token(&form);

    let (status, body, _) = app
        .post_form(
            "/services/new",
            &csrf,
            &[
                ("name", "  "),
                ("description", "Handles checkout and refunds."),
                ("visibility", "everyone"),
            ],
        )
        .await;

    assert_eq!(status, StatusCode::OK, "should re-render the form");
    assert!(
        body.contains("Handles checkout and refunds."),
        "description should survive the rejection"
    );
    assert!(
        body.contains(r#"action="/services/new""#),
        "the form should still post to the creation route"
    );
    assert!(
        body.contains(r#"id="name-error""#) && !body.contains(r#"id="form-error""#),
        "a refused name is said under the name field, not above the form"
    );
}

#[tokio::test]
async fn a_service_saved_with_a_check_is_watched() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let csrf = app.csrf_from("/services/new").await;

    let (status, _, _) = app
        .post_form(
            "/services/new",
            &csrf,
            &[
                ("name", "Intranet"),
                ("check_kind", "http"),
                ("check_url", " https://intranet.example.com "),
                ("check_host", "left over from the other kind"),
                ("check_internal_cert", "on"),
                ("visibility", "everyone"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let checked = ServiceRepository::list_checked(&app.pool).await.unwrap();
    assert_eq!(checked.len(), 1);
    assert_eq!(checked[0].kind, CheckKind::Http);
    assert_eq!(checked[0].target, "https://intranet.example.com");
    assert!(checked[0].internal_cert);

    let (_, list) = app.get(&format!("/services?saved={}", checked[0].id)).await;
    assert!(
        list.contains(&text_with_minutes(
            "services.saved_monitored_notice",
            &checked[0].outage_after_minutes.to_string()
        )),
        "the notice says what monitoring means"
    );

    let (_, form) = app.get(&format!("/services/{}/edit", checked[0].id)).await;
    assert!(form.contains(r#"value="https://intranet.example.com""#));
    assert!(form.contains(r#"value="http" class="sr-only" checked"#));
}

#[tokio::test]
async fn a_refused_check_saves_nothing_and_is_said_under_its_field() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let csrf = app.csrf_from("/services/new").await;

    let (status, body, _) = app
        .post_form(
            "/services/new",
            &csrf,
            &[
                ("name", "NAS"),
                ("check_kind", "tcp"),
                ("check_host", "nas"),
                ("check_port", ""),
            ],
        )
        .await;

    assert_eq!(status, StatusCode::OK, "should re-render the form");
    assert!(body.contains(r#"id="check-error""#) && !body.contains(r#"id="form-error""#));
    assert!(
        body.contains(r#"value="nas""#),
        "the typed host should survive"
    );
    assert!(body.contains("Saisissez un port entre 1 et 65535"));
    assert!(
        ServiceRepository::list_all(&app.pool)
            .await
            .unwrap()
            .is_empty(),
        "a refused check should save no service"
    );
}

#[tokio::test]
async fn removing_the_check_clears_a_detected_outage() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let id = app.create_service("Paie").await;
    let check = ServiceCheck {
        kind: CheckKind::Tcp,
        target: "10.0.0.9:443".to_string(),
        internal_cert: false,
    };
    ServiceRepository::set_check(&app.pool, id, Some(&check))
        .await
        .unwrap();
    ServiceRepository::mark_detected_down(&app.pool, id, Utc::now(), &OutageCause::Report)
        .await
        .unwrap();
    ServiceService::recalculate_status(&app.pool, id)
        .await
        .unwrap();
    assert_eq!(app.service(id).await.status, ServiceStatus::MajorOutage);

    let csrf = app.csrf_from(&format!("/services/{id}/edit")).await;
    let (status, _, _) = app
        .post_form(
            &format!("/services/{id}/edit"),
            &csrf,
            &[
                ("name", "Paie"),
                ("check_kind", "none"),
                ("visibility", "everyone"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let service = app.service(id).await;
    assert_eq!(service.status, ServiceStatus::Operational);
    assert_eq!(service.detected_status, None);
    assert!(
        ServiceRepository::list_checked(&app.pool)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn testing_a_check_says_why_it_fails_and_saves_nothing() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
        .to_string();
    let csrf = app.csrf_from("/services/new").await;

    let (status, body, _) = app
        .post_form(
            "/services/check-test",
            &csrf,
            &[
                ("name", ""),
                ("check_kind", "tcp"),
                ("check_host", "127.0.0.1"),
                ("check_port", &closed),
            ],
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"data-tone="crit""#), "a failure is red");
    assert!(body.contains("Port fermé"), "the reason is said in words");
    assert!(
        !body.contains("Sans réponse pendant"),
        "a failure says nothing of the rule"
    );
    assert!(body.contains("Détail technique"), "the raw error follows");
    assert!(
        ServiceRepository::list_all(&app.pool)
            .await
            .unwrap()
            .is_empty(),
        "a test saves nothing"
    );
}

#[tokio::test]
async fn testing_an_open_port_says_it_answers() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let csrf = app.csrf_from("/services/new").await;

    let (status, body, _) = app
        .post_form(
            "/services/check-test",
            &csrf,
            &[
                ("check_kind", "tcp"),
                ("check_host", "127.0.0.1"),
                ("check_port", &app.addr.port().to_string()),
                ("check_outage_after", "15"),
            ],
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"data-tone="ok""#));
    assert!(body.contains("Le port accepte la connexion"));
    assert!(
        body.contains(&text_with_minutes("services.check_result_rule", "15")),
        "an answer says what the monitoring will do, with the delay typed"
    );
}

#[tokio::test]
async fn testing_with_a_wrong_delay_says_so() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let csrf = app.csrf_from("/services/new").await;

    let (status, body, _) = app
        .post_form(
            "/services/check-test",
            &csrf,
            &[
                ("check_kind", "tcp"),
                ("check_host", "127.0.0.1"),
                ("check_port", &app.addr.port().to_string()),
                ("check_outage_after", "90"),
            ],
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"class="field-error""#));
    assert!(body.contains("Saisissez un nombre de minutes entre 1 et 60"));
}

#[tokio::test]
async fn each_service_keeps_the_delay_typed_for_it() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let csrf = app.csrf_from("/services/new").await;
    let (_, form) = app.get("/services/new").await;
    assert!(
        form.contains(r#"name="check_outage_after""#) && form.contains(r#"value="3""#),
        "a new service offers the delay, three minutes by default"
    );

    let fields = |delay: &'static str| {
        [
            ("name", "Imprimante"),
            ("check_kind", "tcp"),
            ("check_host", "10.0.0.7"),
            ("check_port", "9100"),
            ("check_outage_after", delay),
            ("visibility", "everyone"),
        ]
    };
    let (status, _, _) = app.post_form("/services/new", &csrf, &fields("15")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let checked = ServiceRepository::list_checked(&app.pool).await.unwrap();
    assert_eq!(checked[0].outage_after_minutes, 15);
    let id = checked[0].id;

    let (_, list) = app.get(&format!("/services?saved={id}")).await;
    assert!(
        list.contains(&text_with_minutes("services.saved_monitored_notice", "15")),
        "the notice gives the service's own delay"
    );
    let (_, form) = app.get(&format!("/services/{id}/edit")).await;
    assert!(form.contains(r#"value="15""#), "the saved delay is shown");

    let edit = format!("/services/{id}/edit");
    let (status, _, _) = app.post_form(&edit, &csrf, &fields(" ")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        app.service(id).await.outage_after_minutes,
        3,
        "empty means the default"
    );

    app.post_form(&edit, &csrf, &fields("20")).await;
    app.post_form(
        &edit,
        &csrf,
        &[
            ("name", "Imprimante"),
            ("check_kind", "none"),
            ("check_outage_after", "left over"),
            ("visibility", "everyone"),
        ],
    )
    .await;
    assert_eq!(
        app.service(id).await.outage_after_minutes,
        20,
        "turning the monitoring off keeps the delay, whatever its box holds"
    );
}

#[tokio::test]
async fn a_refused_delay_is_said_under_its_own_field() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let csrf = app.csrf_from("/services/new").await;

    for delay in ["0", "61", "abc"] {
        let (status, body, _) = app
            .post_form(
                "/services/new",
                &csrf,
                &[
                    ("name", "Paie"),
                    ("check_kind", "http"),
                    ("check_url", "https://paie.example.com"),
                    ("check_outage_after", delay),
                ],
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{delay}: the form is shown again");
        assert!(body.contains(r#"id="outage-error""#), "{delay}");
        assert!(
            body.contains(r#"aria-invalid="true" aria-describedby="outage-error""#),
            "{delay}: the delay field is marked wrong and points at its message"
        );
        assert!(
            !body.contains(r#"id="check-error""#) && !body.contains(r#"id="form-error""#),
            "{delay}: the address is not blamed"
        );
        assert!(
            !body.contains(r#"aria-invalid="true" aria-describedby="check-error""#),
            "{delay}: the address field is not marked wrong"
        );
        assert!(
            body.contains(&format!(r#"value="{delay}""#)),
            "{delay}: the typed delay stays"
        );
    }
    assert!(
        ServiceRepository::list_all(&app.pool)
            .await
            .unwrap()
            .is_empty(),
        "a refused delay saves no service"
    );
}

#[tokio::test]
async fn testing_a_misspelt_address_says_what_is_wrong() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let csrf = app.csrf_from("/services/new").await;

    let (status, body, _) = app
        .post_form(
            "/services/check-test",
            &csrf,
            &[
                ("check_kind", "tcp"),
                ("check_host", "nas"),
                ("check_port", ""),
            ],
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"class="field-error""#));
    assert!(body.contains("Saisissez un port entre 1 et 65535"));
}

#[tokio::test]
async fn readers_cannot_test_a_check() {
    let app = TestApp::spawn().await;
    app.create_user(
        "reader@example.com",
        "reader_password_12",
        "Reader",
        Role::Reader,
    )
    .await;
    app.login("reader@example.com", "reader_password_12").await;
    let csrf = app.csrf_from("/").await;

    let (status, _, _) = app
        .post_form(
            "/services/check-test",
            &csrf,
            &[
                ("check_kind", "tcp"),
                ("check_host", "127.0.0.1"),
                ("check_port", &app.addr.port().to_string()),
            ],
        )
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// Sets a port check on a service and records that the checks found it
/// down at `started_at`, as the background task would.
async fn detect_outage(app: &TestApp, id: i64, started_at: chrono::DateTime<Utc>) {
    let check = ServiceCheck {
        kind: CheckKind::Tcp,
        target: "10.0.0.9:443".to_string(),
        internal_cert: false,
    };
    ServiceRepository::set_check(&app.pool, id, Some(&check))
        .await
        .unwrap();
    ServiceRepository::mark_detected_down(&app.pool, id, started_at, &OutageCause::Report)
        .await
        .unwrap();
    ServiceService::recalculate_status(&app.pool, id)
        .await
        .unwrap();
}

#[tokio::test]
async fn visitors_read_that_an_outage_was_detected_automatically() {
    let app = TestApp::spawn_public().await;
    let id = app.create_service("Messagerie").await;
    detect_outage(&app, id, Utc::now() - chrono::Duration::minutes(14)).await;

    let (_, page) = app.get("/").await;
    assert!(page.contains("Détecté automatiquement"));
    assert!(page.contains("depuis 14"), "the banner says for how long");
    assert!(
        !page.contains("/events/new?service="),
        "visitors cannot declare"
    );

    let (_, panel) = app.get(&format!("/services/{id}/drawer")).await;
    assert!(panel.contains("détecté automatiquement"));
}

#[tokio::test]
async fn editors_declare_the_incident_of_a_detected_outage_from_the_banner() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let id = app.create_service("Paie").await;
    let started = Utc::now() - chrono::Duration::minutes(20);
    detect_outage(&app, id, started).await;

    let (_, page) = app.get("/").await;
    assert!(page.contains(&format!(r#"href="/events/new?service={id}""#)));
    assert!(page.contains("Déclarer un incident"));

    let (status, form) = app.get(&format!("/events/new?service={id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        form.contains(&format!(
            r#"value="{id}" class="checkbox" data-service-name="Paie" checked"#
        )),
        "the service is ticked"
    );
    assert!(form.contains(r#"value="critical" class="sr-only" checked"#));
    let began = statup::clock::format_input(&started);
    assert!(
        form.contains(&format!(
            r#"name="started_at" type="hidden" value="{began}""#
        )),
        "the incident begins with the outage"
    );

    let (_, services) = app.get("/services").await;
    assert!(
        services.contains(r#"class="svc-pulse""#),
        "a monitored service shows its pulse"
    );
    assert!(services.contains("Détecté automatiquement"));
}

#[tokio::test]
async fn a_declared_incident_replaces_the_detection_note() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let id = app.create_service("Wiki").await;
    detect_outage(&app, id, Utc::now()).await;

    app.submit_create_event(
        vec![
            ("kind", "incident".to_string()),
            ("severity", "critical".to_string()),
            ("title", "Le wiki ne répond plus".to_string()),
            ("description", String::new()),
        ],
        &[id],
    )
    .await;

    let (_, page) = app.get("/").await;
    assert!(page.contains("Le wiki ne répond plus"));
    assert!(!page.contains("Détecté automatiquement"));
}

#[tokio::test]
async fn a_detected_outage_of_42_minutes_shows_in_the_strip() {
    let app = TestApp::spawn_public().await;
    let id = app.create_service("Messagerie").await;
    let day_start = statup::clock::day_start(statup::clock::today()).unwrap();
    ServiceRepository::mark_detected_down(&app.pool, id, day_start, &OutageCause::Report)
        .await
        .unwrap();
    ServiceRepository::mark_detected_up(&app.pool, id, day_start + chrono::Duration::minutes(42))
        .await
        .unwrap();

    let (_, page) = app.get("/").await;
    assert!(page.contains("data-day-status=\"Sans réponse pendant 42\u{a0}min\""));
}

/// Answers for the service named, fails for the others.
struct AnswersOnly(i64);

impl Prober for AnswersOnly {
    fn check(&self, service: &CheckedService) -> impl Future<Output = Report> + Send {
        let finding = if service.id == self.0 {
            Finding::Answered { status: Some(200) }
        } else {
            Finding::Refused
        };
        std::future::ready(Report {
            finding,
            detail: None,
            elapsed: std::time::Duration::from_millis(84),
        })
    }
}

#[tokio::test]
async fn the_services_page_shows_what_the_last_check_found() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let site = app.create_service("Syspirit").await;
    let vps = app.create_service("VPS").await;
    for id in [site, vps] {
        let check = ServiceCheck {
            kind: CheckKind::Http,
            target: format!("https://service-{id}.example"),
            internal_cert: false,
        };
        ServiceRepository::set_check(&app.pool, id, Some(&check))
            .await
            .unwrap();
    }

    let (_, before) = app.get("/services").await;
    assert!(
        before.contains("première vérification dans la minute"),
        "the pulse waits for the first round"
    );

    let mut state = MonitorState::sharing(std::sync::Arc::clone(&app.checks));
    run_round(
        &app.pool,
        &std::sync::Arc::new(AnswersOnly(site)),
        &mut state,
        Utc::now(),
    )
    .await
    .unwrap();

    let (_, after) = app.get("/services").await;
    assert!(after.contains("84\u{a0}ms"), "an answer shows its latency");
    assert!(after.contains("sans réponse"), "a failure says so");
    assert!(!after.contains("première vérification"));
}

#[tokio::test]
async fn feed_is_private_when_public_mode_is_off() {
    let app = TestApp::spawn().await;

    let (status, _) = app.get("/feed").await;
    assert_eq!(
        status,
        StatusCode::SEE_OTHER,
        "anonymous feed should redirect"
    );
}

#[tokio::test]
async fn feed_lists_events_with_their_updates() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Checkout API").await;

    let path = app
        .create_incident(
            "Payments are failing",
            "Cards are **declined** at checkout",
            "critical",
            &[service_id],
        )
        .await;
    let event_id = event_id_from_path(&path);

    let (_, detail_body) = app.get(&path).await;
    let csrf = extract_csrf_token(&detail_body);
    let (status, _, _) = app
        .post_form(
            &format!("/events/{event_id}/updates"),
            &csrf,
            &[("message", "Provider confirmed the outage")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let resp = app
        .client
        .get(app.url("/feed"))
        .send()
        .await
        .expect("GET /feed failed");
    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.starts_with("application/atom+xml"),
        "feed should be served as Atom, got {content_type}"
    );

    let body = resp.text().await.expect("feed body");
    assert!(body.starts_with("<?xml"), "feed should be an XML document");
    assert!(
        body.contains("<title>Payments are failing</title>"),
        "the incident should be an entry"
    );
    assert!(
        body.contains(&format!("http://{}/events/{event_id}", app.addr)),
        "entry links should be absolute and point at the event page"
    );
    assert!(
        body.contains("Checkout API"),
        "the affected service should be named in the entry"
    );
    assert!(
        body.contains("&#60;strong&#62;declined&#60;/strong&#62;"),
        "the description should be rendered from Markdown and escaped for XML"
    );
    assert!(
        !body.contains("<strong>declined"),
        "rendered Markdown must never reach the feed unescaped"
    );
    assert!(
        body.contains("Provider confirmed the outage"),
        "posted updates should be part of the entry"
    );

    let (_, home) = app.get("/").await;
    assert!(
        home.contains(r#"<link rel="alternate" type="application/atom+xml" href="/feed""#),
        "pages should advertise the feed for auto discovery"
    );
    assert!(
        home.contains(r#"href="/subscribe""#),
        "the banner should offer the subscription page"
    );
}

#[tokio::test]
async fn an_open_incident_says_how_long_ago_its_last_update_was_posted() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Search").await;
    let path = app
        .create_incident(
            "Search is slow",
            "Latency above 2s",
            "critical",
            &[service_id],
        )
        .await;
    let event_id = event_id_from_path(&path);

    let (_, body) = app.get(&path).await;
    assert!(
        !body.contains("dernière mise à jour"),
        "without an update, the time since it opened already says it all"
    );

    app.post_form_with_header_csrf(
        &format!("/events/{event_id}/updates"),
        &[("lifecycle", ""), ("message", "Supplier on it")],
    )
    .await;
    let (_, body) = app.get(&path).await;
    assert!(
        body.contains("dernière mise à jour il y a 1\u{a0}min"),
        "the state line should tell how old the last word is"
    );
}

#[tokio::test]
async fn detail_page_says_an_incident_is_still_ongoing() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Search").await;

    let path = app
        .create_incident(
            "Search is slow",
            "Latency above 2s",
            "critical",
            &[service_id],
        )
        .await;
    let event_id = event_id_from_path(&path);

    let (_, body) = app.get(&path).await;
    assert!(
        body.contains(r#"class="fact-state" data-tone="crit""#),
        "an open incident should carry its tone on its own page"
    );
    assert!(
        body.contains("ouvert depuis"),
        "the time since the incident opened should be shown"
    );
    assert!(
        body.contains(r#"name="lifecycle""#) && body.contains(r#"<option value="">"#),
        "the state list should open on keeping the current state"
    );

    let (status, body, _) = app
        .post_form_with_header_csrf(
            &format!("/events/{event_id}/updates"),
            &[("lifecycle", ""), ("message", "")],
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an update with nothing in it is refused in the page"
    );
    assert!(body.contains("Écrivez un message"));

    let (status, _, location) = app
        .post_form_with_header_csrf(
            &format!("/events/{event_id}/updates"),
            &[("lifecycle", "resolved"), ("message", "Index rebuilt")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some(format!("/events/{event_id}?published=1").as_str()),
        "back on the event, where the update lights up once"
    );
    let (_, body) = app.get(&format!("/events/{event_id}?published=1")).await;
    assert!(
        body.contains("Événement publié."),
        "the editor sees the update went out"
    );
    assert!(
        body.contains("seul un administrateur peut encore le modifier"),
        "a closed incident tells the editor why its buttons are gone"
    );

    let (_, body) = app.get(&path).await;
    assert!(
        !body.contains(r#"class="fact-state" data-tone="crit""#),
        "a closed incident should not claim to be ongoing"
    );
    assert!(
        body.contains("Index rebuilt"),
        "the closing message is shown"
    );
}

#[tokio::test]
async fn a_finished_maintenance_offers_the_announcement_of_what_is_new() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Payroll").await;
    let path = app
        .create_planned_maintenance("Payroll update", "New version", "minor", &[service_id])
        .await;
    let event_id = event_id_from_path(&path);

    let (_, body) = app.get(&path).await;
    assert!(
        !body.contains(&format!("after={event_id}")),
        "nothing to announce before the work is done"
    );

    for lifecycle in ["in_progress", "completed"] {
        let (status, _, _) = app
            .post_form_with_header_csrf(
                &format!("/events/{event_id}/updates"),
                &[("lifecycle", lifecycle), ("message", "Done")],
            )
            .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
    }

    let (_, body) = app.get(&path).await;
    assert!(
        body.contains(&format!(
            "/events/new?kind=publication&amp;after={event_id}"
        )),
        "a finished maintenance offers the announcement: {body}"
    );

    let (status, body) = app
        .get(&format!("/events/new?kind=publication&after={event_id}"))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("Payroll update"),
        "the title names the maintenance"
    );
    assert!(
        body.contains(&format!(
            "<option value=\"{event_id}\" selected>Payroll update\u{a0}· "
        )),
        "the maintenance is preselected, with the day it ended: {body}"
    );
    assert!(
        body.contains(r#"value="changelog" class="sr-only" checked"#)
            || body.contains(r#"value="changelog" class="sr-only" data-required checked"#),
        "the announcement is a changelog: {body}"
    );
}

#[tokio::test]
async fn an_announcement_names_the_maintenance_it_follows() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Payroll").await;
    let path = app
        .create_planned_maintenance("Payroll update", "New version", "minor", &[service_id])
        .await;
    let maintenance_id = event_id_from_path(&path);
    for lifecycle in ["in_progress", "completed"] {
        app.post_form_with_header_csrf(
            &format!("/events/{maintenance_id}/updates"),
            &[("lifecycle", lifecycle), ("message", "Done")],
        )
        .await;
    }

    let announcement = app
        .submit_create_event(
            vec![
                ("title", "What is new".to_string()),
                ("description", "Faster payslips".to_string()),
                ("kind", "publication".to_string()),
                ("category", "changelog".to_string()),
                ("follows_event_id", maintenance_id.to_string()),
            ],
            &[service_id],
        )
        .await;
    let (status, body) = app.get(&announcement).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(&format!(r#"href="/events/{maintenance_id}""#))
            && body.contains("Payroll update"),
        "the announcement links to the maintenance it follows: {body}"
    );
}

#[tokio::test]
async fn a_state_set_by_hand_outlasts_the_events_on_its_service() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("HR portal").await;
    app.set_state_by_hand(service_id, "major_outage").await;
    assert_eq!(
        app.service(service_id).await.status,
        ServiceStatus::MajorOutage
    );

    let incident = app
        .create_incident("HR portal slow", "Pages take long", "minor", &[service_id])
        .await;
    let incident_id = event_id_from_path(&incident);
    assert_eq!(
        app.service(service_id).await.status,
        ServiceStatus::MajorOutage,
        "a milder incident does not lower what the team declared"
    );
    app.post_form_with_header_csrf(
        &format!("/events/{incident_id}/updates"),
        &[("lifecycle", "resolved"), ("message", "Back to normal")],
    )
    .await;
    assert_eq!(
        app.service(service_id).await.status,
        ServiceStatus::MajorOutage,
        "closing an incident falls back to the state set by hand"
    );

    app.set_state_by_hand(service_id, "degraded").await;
    let path = app
        .create_planned_maintenance("Upgrade", "New version", "minor", &[service_id])
        .await;
    let maintenance_id = event_id_from_path(&path);
    app.post_form_with_header_csrf(
        &format!("/events/{maintenance_id}/updates"),
        &[("lifecycle", "in_progress")],
    )
    .await;
    assert_eq!(
        app.service(service_id).await.status,
        ServiceStatus::Degraded
    );
    app.post_form_with_header_csrf(
        &format!("/events/{maintenance_id}/updates"),
        &[("lifecycle", "completed"), ("message", "Done")],
    )
    .await;
    assert_eq!(
        app.service(service_id).await.status,
        ServiceStatus::Degraded,
        "a finished maintenance leaves the state set by hand"
    );

    app.set_state_by_hand(service_id, "operational").await;
    app.create_incident("HR portal down", "No page loads", "critical", &[service_id])
        .await;
    let service = app.service(service_id).await;
    assert_eq!(service.manual_status, ServiceStatus::Operational);
    assert_eq!(service.status, ServiceStatus::MajorOutage);
    let (_, list) = app.get("/services").await;
    assert!(
        list.contains(">HR portal down</a>")
            && !list.contains(&format!(r#"id="status-{service_id}""#)),
        "the list names the event that sets the state, instead of the control"
    );
}

#[tokio::test]
async fn availability_ends_when_the_service_came_back() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Payroll").await;
    let path = app
        .create_incident("Payroll down", "No access", "critical", &[service_id])
        .await;
    let event_id = event_id_from_path(&path);
    app.post_form_with_header_csrf(
        &format!("/events/{event_id}/updates"),
        &[("lifecycle", "monitoring")],
    )
    .await;

    let since = chrono::Utc::now() - chrono::Duration::days(30);
    let spans = statup::repositories::EventRepository::incident_spans(&app.pool, since)
        .await
        .expect("db error");
    let span = &spans[&service_id][0];
    assert!(
        span.end.is_some(),
        "an incident under watch no longer counts as down time"
    );
}

#[tokio::test]
async fn a_maintenance_without_a_start_begins_right_away() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("File server").await;
    app.submit_create_event(
        vec![
            ("title", "Disk replacement".to_string()),
            ("kind", "maintenance".to_string()),
            ("planned_start", String::new()),
        ],
        &[service_id],
    )
    .await;
    assert_eq!(
        app.service(service_id).await.status,
        ServiceStatus::Maintenance
    );
}

#[tokio::test]
async fn a_maintenance_begun_right_away_keeps_its_end() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("File server").await;
    let end = chrono::Utc::now() + chrono::TimeDelta::hours(1);
    let location = app
        .submit_create_event(
            vec![
                ("title", "Disk replacement".to_string()),
                ("kind", "maintenance".to_string()),
                ("planned_start", String::new()),
                ("planned_end", statup::clock::format_input(&end)),
            ],
            &[service_id],
        )
        .await;
    let id: i64 = location
        .trim_start_matches("/events/")
        .split('?')
        .next()
        .and_then(|id| id.parse().ok())
        .expect("event id in the redirect");
    let event = statup::repositories::EventRepository::find_by_id(&app.pool, id)
        .await
        .expect("db error")
        .expect("event not found");
    assert!(!event.planned);
    assert_eq!(
        event
            .planned_end
            .map(|end| statup::clock::format_input(&end)),
        Some(statup::clock::format_input(&end))
    );
}

#[tokio::test]
async fn an_incident_declared_late_keeps_when_it_began() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Mail").await;
    let began = chrono::Utc::now() - chrono::TimeDelta::minutes(40);
    let location = app
        .submit_create_event(
            vec![
                ("title", "Mail down".to_string()),
                ("kind", "incident".to_string()),
                ("severity", "critical".to_string()),
                ("started_at", statup::clock::format_input(&began)),
            ],
            &[service_id],
        )
        .await;
    let event =
        statup::repositories::EventRepository::find_by_id(&app.pool, event_id_from_path(&location))
            .await
            .expect("db error")
            .expect("event not found");
    assert_eq!(
        event.started_at.map(|at| statup::clock::format_input(&at)),
        Some(statup::clock::format_input(&began))
    );

    let later = chrono::Utc::now() + chrono::TimeDelta::hours(2);
    let csrf = app.csrf_from("/events/new").await;
    let started_at = statup::clock::format_input(&later);
    let (status, body, _) = app
        .post_form(
            "/events/new",
            &csrf,
            &[
                ("title", "Mail slow"),
                ("kind", "incident"),
                ("severity", "minor"),
                ("started_at", started_at.as_str()),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::OK, "refused in the form");
    assert!(body.contains("form-error"));
}

#[tokio::test]
async fn an_update_posted_from_the_side_panel_redraws_it() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("VPN").await;
    let location = app
        .create_incident("VPN down", "", "critical", &[service_id])
        .await;
    let id = event_id_from_path(&location);
    let path = format!("/events/{id}/panel-updates");

    let csrf = app.csrf_from(&format!("/events/{id}")).await;
    let (status, body, _) = app
        .post_form(
            &path,
            &csrf,
            &[("message", "Provider called"), ("lifecycle", "in_progress")],
        )
        .await;
    assert_eq!(status, StatusCode::OK, "the panel is drawn in place");
    assert!(body.contains("drawer-title"));
    assert!(body.contains("Provider called"));

    let (status, body, _) = app.post_form(&path, &csrf, &[("message", "")]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("form-error") && body.contains("composer is-open"),
        "a refused update keeps the composer open with its reason"
    );
}

#[tokio::test]
async fn a_service_panel_tells_its_story() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Payroll").await;
    app.create_incident("Payroll slow", "", "minor", &[service_id])
        .await;

    let (status, body) = app.get(&format!("/services/{service_id}/drawer")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("drawer-title") && body.contains("Payroll"));
    assert!(body.contains("Payroll slow"), "its last events are listed");
    assert!(body.contains(&format!("/events?service_id={service_id}")));

    let printers = app.create_service("Printers").await;
    app.create_incident("Printers jammed", "", "minor", &[printers])
        .await;
    let (status, body) = app.get(&format!("/events?service_id={service_id}")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the link to its events opens the list"
    );
    assert!(body.contains("Payroll slow"));
    assert!(!body.contains("Printers jammed"), "only its own events");
}

#[tokio::test]
async fn editors_reach_a_service_settings_from_its_row_and_its_panel() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let id = app.create_service("Payroll").await;
    let edit = format!(r#"href="/services/{id}/edit""#);

    let (_, page) = app.get("/services").await;
    assert!(page.contains(&edit) && page.contains(r#"aria-label="Modifier Payroll""#));

    let (_, panel) = app.get(&format!("/services/{id}/drawer")).await;
    assert!(panel.contains(&edit), "the panel offers the pencil");
}

#[tokio::test]
async fn service_history_spans_ninety_days_where_there_is_room() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let id = app.create_service("Payroll").await;

    let (_, home) = app.get("/").await;
    assert_eq!(home.matches("data-day-date=").count(), 90);
    assert_eq!(
        home.matches("bar-early").count(),
        60,
        "a narrow column hides them"
    );
    assert!(home.contains("30 derniers jours\u{a0}:") && home.contains("90 derniers jours\u{a0}:"));

    let (_, page) = app.get("/services").await;
    assert!(page.contains("svc-history") && page.contains("en 90 jours"));
    assert!(
        page.contains("en 30 jours"),
        "a phone keeps the last thirty"
    );

    let (_, panel) = app.get(&format!("/services/{id}/drawer")).await;
    assert_eq!(panel.matches("data-day-date=").count(), 30);
    assert!(
        panel.contains(">30 derniers jours<"),
        "the panel keeps its month"
    );
}

#[tokio::test]
async fn a_phone_folds_the_filters_unless_one_is_set() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let id = app.create_service("Payroll").await;
    app.create_incident("Payroll slow", "", "minor", &[id])
        .await;

    let (_, plain) = app.get("/events").await;
    assert!(plain.contains(r#"class="filters-more""#), "folded");
    assert!(plain.contains(r#"aria-expanded="false""#));

    let (_, filtered) = app.get(&format!("/events?service_id={id}&q=slow")).await;
    assert!(
        filtered.contains(r#"class="filters-more is-open""#),
        "opened"
    );
    assert!(
        filtered.contains(r#"data-filters-count>1<"#),
        "the search is not counted"
    );
}

#[tokio::test]
async fn visitors_get_no_pencil_in_a_service_panel() {
    let app = TestApp::spawn_public().await;
    let id = app.create_service("Payroll").await;

    let (status, panel) = app.get(&format!("/services/{id}/drawer")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!panel.contains(&format!("/services/{id}/edit")));
}

async fn lifecycle_of(app: &TestApp, event_id: i64) -> Option<Lifecycle> {
    EventRepository::find_by_id(&app.pool, event_id)
        .await
        .expect("db error")
        .expect("event not found")
        .lifecycle
}

#[tokio::test]
async fn deleting_an_ongoing_incident_gives_its_services_back() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Mail").await;
    let path = app
        .create_incident("Mail down", "No mail goes out", "critical", &[service_id])
        .await;
    let event_id = event_id_from_path(&path);
    assert_eq!(
        app.service(service_id).await.status,
        ServiceStatus::MajorOutage
    );

    let (status, _, location) = app
        .post_form_with_header_csrf(&format!("/events/{event_id}/delete"), &[])
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location.as_deref(), Some("/events"));
    assert_eq!(
        app.service(service_id).await.status,
        ServiceStatus::Operational
    );
    let (status, _) = app.get(&path).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn undoing_a_step_puts_its_services_back_once() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("VPN").await;
    let path = app
        .create_incident("VPN drops", "Sessions drop", "critical", &[service_id])
        .await;
    let event_id = event_id_from_path(&path);
    let (status, _, _) = app
        .post_form_with_header_csrf(
            &format!("/events/{event_id}/updates"),
            &[("lifecycle", "monitoring")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        app.service(service_id).await.status,
        ServiceStatus::Operational,
        "an incident under watch leaves its services up"
    );

    let revert = format!("/events/{event_id}/revert-lifecycle");
    let (status, _, _) = app.post_form_with_header_csrf(&revert, &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        lifecycle_of(&app, event_id).await,
        Some(Lifecycle::Investigating)
    );
    assert_eq!(
        app.service(service_id).await.status,
        ServiceStatus::MajorOutage
    );

    let (status, body, _) = app.post_form_with_header_csrf(&revert, &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "one step back only");
    assert!(body.contains("état précédent à rétablir"), "{body}");
}

#[tokio::test]
async fn a_closed_event_belongs_to_administrators() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Intranet").await;
    let path = app
        .create_incident("Intranet slow", "Pages are slow", "minor", &[service_id])
        .await;
    let event_id = event_id_from_path(&path);
    let (status, _, _) = app
        .post_form_with_header_csrf(
            &format!("/events/{event_id}/updates"),
            &[("lifecycle", "resolved")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let csrf = app.csrf_from("/").await;
    let (status, body, _) = app
        .post_form(
            &format!("/events/{event_id}/edit"),
            &csrf,
            &[
                ("title", "Rewritten"),
                ("description", "Rewritten"),
                ("severity", "critical"),
                ("service_ids", &service_id.to_string()),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "edit");
    assert!(body.contains("seul un administrateur"), "{body}");
    for action in ["delete", "revert-lifecycle"] {
        let (status, _, _) = app
            .post_form_with_header_csrf(&format!("/events/{event_id}/{action}"), &[])
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{action}");
    }
    let (status, _, _) = app
        .post_form_with_header_csrf(
            &format!("/events/{event_id}/updates"),
            &[("message", "One more thing")],
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "update");
    let (status, _) = app.get(&format!("/events/{event_id}/edit")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "edit form");

    let event = EventRepository::find_by_id(&app.pool, event_id)
        .await
        .expect("db error")
        .expect("the event is still there");
    assert_eq!(event.title, "Intranet slow");
    assert_eq!(event.lifecycle, Some(Lifecycle::Resolved));
    assert_eq!(
        app.service(service_id).await.status,
        ServiceStatus::Operational
    );
}

#[tokio::test]
async fn an_administrator_can_still_edit_a_closed_event() {
    let app = TestApp::spawn().await;
    app.create_user("admin@example.com", "admin_pass_1234", "Admin", Role::Admin)
        .await;
    app.login("admin@example.com", "admin_pass_1234").await;
    let service_id = app.create_service("Intranet").await;
    let path = app
        .create_incident("Intranet slow", "Pages are slow", "minor", &[service_id])
        .await;
    let event_id = event_id_from_path(&path);
    let (status, _, _) = app
        .post_form_with_header_csrf(
            &format!("/events/{event_id}/updates"),
            &[("lifecycle", "resolved")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let csrf = app.csrf_from("/").await;
    let (status, _, location) = app
        .post_form(
            &format!("/events/{event_id}/edit"),
            &csrf,
            &[
                ("title", "Rewritten"),
                ("description", "Rewritten"),
                ("severity", "minor"),
                ("service_ids", &service_id.to_string()),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some(format!("/events/{event_id}").as_str())
    );

    let event = EventRepository::find_by_id(&app.pool, event_id)
        .await
        .expect("db error")
        .expect("the event is still there");
    assert_eq!(event.title, "Rewritten");
}

#[tokio::test]
async fn a_publisher_edits_a_service() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Billing").await;

    let (status, form) = app.get(&format!("/services/{service_id}/edit")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(form.contains("Billing"), "the form starts from the name");
    let (status, _) = app.get("/services/9999/edit").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown service");

    let (status, _, location) = app
        .post_form(
            &format!("/services/{service_id}/edit"),
            &extract_csrf_token(&form),
            &[
                ("name", "Billing API"),
                ("description", "Handles invoices."),
                ("visibility", "everyone"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some(format!("/services?saved={service_id}").as_str())
    );
    let service = app.service(service_id).await;
    assert_eq!(service.name, "Billing API");
    assert_eq!(service.description.as_deref(), Some("Handles invoices."));
}

#[tokio::test]
async fn an_empty_name_is_refused_on_the_service_edit_form() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Billing").await;
    let csrf = app.csrf_from(&format!("/services/{service_id}/edit")).await;

    let (status, body, _) = app
        .post_form(
            &format!("/services/{service_id}/edit"),
            &csrf,
            &[
                ("name", "  "),
                ("description", "Handles invoices."),
                ("visibility", "everyone"),
            ],
        )
        .await;

    assert_eq!(status, StatusCode::OK, "should re-render the form");
    assert!(
        body.contains("Handles invoices."),
        "description should survive the rejection"
    );
    assert!(
        body.contains(r#"id="name-error""#),
        "the refusal is said under the name field"
    );
    let service = app.service(service_id).await;
    assert_eq!(service.name, "Billing");
    assert_eq!(service.description, None);
}

#[tokio::test]
async fn a_service_is_deleted_unless_events_cite_it() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let idle_id = app.create_service("Idle").await;
    let cited_id = app.create_service("Intranet").await;
    app.create_incident("Intranet slow", "Pages are slow", "minor", &[cited_id])
        .await;

    let (status, body, _) = app
        .post_form_with_header_csrf(&format!("/services/{cited_id}/delete"), &[])
        .await;
    assert_eq!(status, StatusCode::OK, "the list is redrawn");
    assert!(
        body.contains("Ce service est cité par des événements\u{a0}: il est conservé."),
        "the refusal is said in the list"
    );
    let kept = ServiceRepository::find_by_id(&app.pool, cited_id)
        .await
        .expect("db error");
    assert!(kept.is_some(), "a service with history stays");

    let (status, _, location) = app
        .post_form_with_header_csrf(&format!("/services/{idle_id}/delete"), &[])
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location.as_deref(), Some("/services?deleted=1"));
    let gone = ServiceRepository::find_by_id(&app.pool, idle_id)
        .await
        .expect("db error");
    assert!(gone.is_none(), "a service without history is gone");
}

#[tokio::test]
async fn a_reader_cannot_edit_or_delete_a_service() {
    let app = TestApp::spawn().await;
    let service_id = app.create_service("Billing").await;
    app.create_user(
        "reader@example.com",
        "reader_pass_1234",
        "Reader",
        Role::Reader,
    )
    .await;
    app.login("reader@example.com", "reader_pass_1234").await;

    let (status, _) = app.get(&format!("/services/{service_id}/edit")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "edit form");
    for (action, fields) in [
        ("edit", &[("name", "Renamed"), ("description", "")][..]),
        ("delete", &[][..]),
    ] {
        let (status, _, _) = app
            .post_form_with_header_csrf(&format!("/services/{service_id}/{action}"), fields)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{action}");
    }

    let service = app.service(service_id).await;
    assert_eq!(service.name, "Billing");
}

async fn template_id_by_title(app: &TestApp, title: &str) -> Option<i64> {
    sqlx::query_scalar("SELECT id FROM event_templates WHERE title = ?")
        .bind(title)
        .fetch_optional(&app.pool)
        .await
        .expect("template lookup failed")
}

async fn publish_incident_as_template(app: &TestApp, title: &str) -> i64 {
    app.submit_create_event(
        vec![
            ("title", title.to_string()),
            ("description", "Card payments are failing".to_string()),
            ("kind", "incident".to_string()),
            ("severity", "critical".to_string()),
            ("save_as_template", "on".to_string()),
        ],
        &[],
    )
    .await;
    template_id_by_title(app, title)
        .await
        .expect("the template should have been saved")
}

#[tokio::test]
async fn an_incident_saved_as_a_template_can_be_searched_and_read() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let id = publish_incident_as_template(&app, "Payment gateway down").await;

    let (status, body) = app.get("/events/templates/search?q=Payment").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Payment gateway down"), "match on 2+ chars");
    let (status, body) = app.get("/events/templates/search?q=P").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("Payment gateway down"),
        "one char is too few"
    );

    let (status, body) = app.get(&format!("/events/templates/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    let detail: serde_json::Value = serde_json::from_str(&body).expect("detail is JSON");
    assert_eq!(detail["title"], "Payment gateway down");
    assert_eq!(detail["description"], "Card payments are failing");
    assert_eq!(detail["kind"], "incident");
    assert_eq!(detail["severity"], "critical");

    let (status, _) = app.get("/events/templates/999999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn publishing_from_a_template_counts_its_usage() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let id = publish_incident_as_template(&app, "Payment gateway down").await;
    let usage = || async {
        sqlx::query_scalar::<_, i64>("SELECT usage_count FROM event_templates WHERE id = ?")
            .bind(id)
            .fetch_one(&app.pool)
            .await
            .expect("usage lookup failed")
    };
    assert_eq!(usage().await, 0, "saving a template is not using it");

    app.submit_create_event(
        vec![
            ("title", "Payment gateway down again".to_string()),
            ("description", "Same failure".to_string()),
            ("kind", "incident".to_string()),
            ("severity", "critical".to_string()),
            ("template_id", id.to_string()),
        ],
        &[],
    )
    .await;

    assert_eq!(usage().await, 1);
}

#[tokio::test]
async fn an_event_panel_opened_from_a_service_leads_back_to_it() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let service_id = app.create_service("Mail").await;
    let path = app
        .create_incident("Mail is slow", "Queue backlog", "minor", &[service_id])
        .await;
    let event_id = event_id_from_path(&path);

    let (_, panel) = app.get(&format!("/services/{service_id}/drawer")).await;
    assert!(
        panel.contains(&format!("/events/{event_id}/drawer?service={service_id}")),
        "an event opens from its service's panel knowing where it came from"
    );

    let (_, panel) = app
        .get(&format!("/events/{event_id}/drawer?service={service_id}"))
        .await;
    assert!(
        panel.contains(&format!(r#"data-drawer="/services/{service_id}/drawer""#)),
        "the event's panel leads back to the service"
    );
    assert!(panel.contains("Retour à Mail"));

    let (_, panel) = app.get(&format!("/events/{event_id}/drawer")).await;
    assert!(
        !panel.contains(r#"class="back-link""#),
        "opened from elsewhere, there is nothing to go back to"
    );

    app.post_form_with_header_csrf(
        &format!("/events/{event_id}/updates"),
        &[("lifecycle", "resolved"), ("message", "Queue drained")],
    )
    .await;
    let (_, panel) = app.get(&format!("/events/{event_id}/drawer")).await;
    assert!(
        panel.contains(&format!(r#"href="/events/{event_id}#review""#)),
        "a resolved incident's panel offers its report, written on its page"
    );
}

#[tokio::test]
async fn a_template_is_removed_from_the_menu_of_a_new_event() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let first = publish_incident_as_template(&app, "Payment gateway down").await;
    publish_incident_as_template(&app, "Mail relay down").await;

    let (_, body) = app.get("/events/new").await;
    assert!(
        body.contains(&format!(r#"action="/events/templates/{first}/delete""#)),
        "each template in the menu can be removed"
    );

    let (status, _, location) = app
        .post_form_with_header_csrf(&format!("/events/templates/{first}/delete"), &[])
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location.as_deref(), Some("/events/new"));
    assert!(
        template_id_by_title(&app, "Payment gateway down")
            .await
            .is_none()
    );
    assert!(
        template_id_by_title(&app, "Mail relay down")
            .await
            .is_some(),
        "only that one goes"
    );

    let (status, _, _) = app
        .post_form_with_header_csrf(&format!("/events/templates/{first}/delete"), &[])
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "already gone");
}

#[tokio::test]
async fn a_reader_cannot_search_read_or_delete_templates() {
    let app = TestApp::spawn().await;
    let reader_id = app
        .create_user(
            "reader@example.com",
            "reader_pass_1234",
            "Reader",
            Role::Reader,
        )
        .await;
    let template = statup::repositories::EventTemplateRepository::create(
        &app.pool,
        statup::repositories::CreateTemplateInput {
            title: "Payment gateway down",
            description: "Card payments are failing",
            kind: statup::models::Kind::Incident,
            severity: Some(statup::models::Severity::Critical),
            planned: false,
            category: None,
            created_by: reader_id,
        },
    )
    .await
    .expect("failed to create template");
    app.login("reader@example.com", "reader_pass_1234").await;

    let (status, _) = app.get("/events/templates/search?q=Payment").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "search");
    let (status, _) = app.get(&format!("/events/templates/{}", template.id)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "detail");
    let (status, _, _) = app
        .post_form_with_header_csrf(&format!("/events/templates/{}/delete", template.id), &[])
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "delete");

    assert!(
        template_id_by_title(&app, "Payment gateway down")
            .await
            .is_some()
    );
}

const NEXT_UPDATE_NOTE: &str = "prévue\u{a0}";
const NEXT_UPDATE_REFUSAL: &str =
    "Écrivez un délai (30 min, 1 h) ou une heure (14:30) pour la prochaine mise à jour.";

/// Declares an incident that announces its next update in 30 minutes.
async fn declare_incident_with_next_update(app: &TestApp) -> (String, i64) {
    let path = app
        .submit_create_event(
            vec![
                ("title", "Mail is slow".to_string()),
                ("description", "Delivery is delayed".to_string()),
                ("kind", "incident".to_string()),
                ("severity", "critical".to_string()),
                ("next_update", "30 min".to_string()),
            ],
            &[],
        )
        .await;
    let id = event_id_from_path(&path);
    (path, id)
}

async fn post_update(app: &TestApp, id: i64, fields: &[(&str, &str)]) -> (StatusCode, String) {
    let (status, body, _) = app
        .post_form_with_header_csrf(&format!("/events/{id}/updates"), fields)
        .await;
    (status, body)
}

#[tokio::test]
async fn a_declared_incident_tells_when_its_next_update_is_due() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let (path, _) = declare_incident_with_next_update(&app).await;

    let (_, body) = app.get(&path).await;
    assert!(
        body.contains(NEXT_UPDATE_NOTE),
        "the state line should say when the next update is due"
    );
    assert!(
        body.contains("value=\"30 min\""),
        "the composer should offer the delay again"
    );
}

#[tokio::test]
async fn a_blank_next_update_clears_the_announced_one() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let (path, id) = declare_incident_with_next_update(&app).await;

    post_update(
        &app,
        id,
        &[
            ("lifecycle", ""),
            ("message", "Still looking"),
            ("next_update", ""),
        ],
    )
    .await;

    let (_, body) = app.get(&path).await;
    assert!(!body.contains(NEXT_UPDATE_NOTE));
    assert!(!body.contains("value=\"30 min\""));
}

#[tokio::test]
async fn a_time_of_day_is_not_offered_again() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let path = app
        .create_incident("Mail is slow", "", "critical", &[])
        .await;
    let id = event_id_from_path(&path);

    post_update(
        &app,
        id,
        &[
            ("lifecycle", ""),
            ("message", "Back soon"),
            ("next_update", "14:30"),
        ],
    )
    .await;

    let (_, body) = app.get(&path).await;
    assert!(body.contains(NEXT_UPDATE_NOTE));
    assert!(
        body.contains("name=\"next_update\" type=\"text\" class=\"input\" value=\"\""),
        "only a delay is offered again, never a time of day"
    );
}

#[tokio::test]
async fn an_unreadable_next_update_is_refused() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let path = app
        .create_incident("Mail is slow", "", "critical", &[])
        .await;
    let id = event_id_from_path(&path);

    let (_, body) = post_update(
        &app,
        id,
        &[
            ("lifecycle", ""),
            ("message", "Soon"),
            ("next_update", "bientôt"),
        ],
    )
    .await;
    assert!(body.contains(NEXT_UPDATE_REFUSAL));
    assert!(
        body.contains("value=\"bientôt\""),
        "what was typed comes back"
    );

    let (_, body) = app.get(&path).await;
    assert!(!body.contains(NEXT_UPDATE_NOTE));
    assert!(!body.contains("Soon"), "a refused update is not stored");
}

#[tokio::test]
async fn a_missed_next_update_says_how_long_it_is_overdue() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let (path, id) = declare_incident_with_next_update(&app).await;

    let twenty_minutes_ago = (Utc::now() - chrono::Duration::minutes(20))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    sqlx::query("UPDATE events SET next_update_at = ?1 WHERE id = ?2")
        .bind(twenty_minutes_ago)
        .bind(id)
        .execute(&app.pool)
        .await
        .unwrap();

    let (_, body) = app.get(&path).await;
    assert!(
        body.contains("mise à jour attendue depuis 2"),
        "an update past its time is said to be overdue"
    );
    assert!(!body.contains(NEXT_UPDATE_NOTE));
}

#[tokio::test]
async fn resolving_an_incident_drops_its_next_update() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let (path, id) = declare_incident_with_next_update(&app).await;

    post_update(
        &app,
        id,
        &[
            ("lifecycle", "resolved"),
            ("message", "Fixed"),
            ("next_update", "30 min"),
        ],
    )
    .await;

    let (_, body) = app.get(&path).await;
    assert!(!body.contains(NEXT_UPDATE_NOTE));
    assert!(!body.contains("mise à jour attendue"));
}

const REPORT: &str = "Un disque plein a arrêté la paie. On surveille désormais sa place.";

/// An incident declared by the signed-in publisher and resolved at once.
async fn resolved_incident(app: &TestApp) -> (String, i64) {
    let service = app.create_service("Paie").await;
    let path = app
        .create_incident(
            "Panne de la paie",
            "La paie ne répond plus.",
            "critical",
            &[service],
        )
        .await;
    let id = event_id_from_path(&path);
    let (status, _, _) = app
        .post_form_with_header_csrf(
            &format!("/events/{id}/updates"),
            &[("lifecycle", "resolved"), ("message", "La paie répond.")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    (format!("/events/{id}"), id)
}

async fn write_report(app: &TestApp, id: i64, body: &str) -> (StatusCode, String, Option<String>) {
    app.post_form_with_header_csrf(&format!("/events/{id}/review"), &[("body", body)])
        .await
}

#[tokio::test]
async fn an_editor_writes_the_report_of_a_resolved_incident() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let (path, id) = resolved_incident(&app).await;
    let (_, before) = app.get(&path).await;
    assert!(before.contains("Écrire le bilan"));

    let (status, _, location) = write_report(&app, id, REPORT).await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location.as_deref(),
        Some(&*format!("/events/{id}?published=1"))
    );
    let (_, page) = app.get(&format!("{path}?published=1")).await;
    assert!(page.contains("Événement publié."));
    assert!(page.contains("Modifier le bilan"));
    assert!(!page.contains("Écrire le bilan"));
    let report = page.find(REPORT).expect("the report is on the page");
    let timeline = page.find("Chronologie").expect("the timeline follows");
    assert!(report < timeline, "the report comes before the timeline");
    let (_, panel) = app.get(&format!("/events/{id}/drawer")).await;
    assert!(panel.contains(REPORT));
    let (_, list) = app.get("/events").await;
    assert!(list.contains("Résolu\u{a0}· bilan"));
}

#[tokio::test]
async fn the_composer_link_is_a_card_action_once_the_incident_is_resolved() {
    let app = TestApp::spawn().await;
    app.create_user("admin@example.com", "admin_pass_1234", "Admin", Role::Admin)
        .await;
    app.login("admin@example.com", "admin_pass_1234").await;
    let (path, _) = resolved_incident(&app).await;

    let (_, page) = app.get(&path).await;

    let link = r##"href="#composer""##;
    assert_eq!(page.matches(link).count(), 1);
    let timeline_head = page
        .find(r#"id="updates-title""#)
        .expect("the timeline card");
    assert!(
        page.find(link).expect("the link") > timeline_head,
        "the link sits in the timeline card, not in the page head"
    );
    assert!(!page.contains(r##"href="#composer" class="btn btn-primary""##));
}

#[tokio::test]
async fn the_composer_link_is_the_primary_button_of_an_ongoing_incident() {
    let app = TestApp::spawn().await;
    app.create_user("admin@example.com", "admin_pass_1234", "Admin", Role::Admin)
        .await;
    app.login("admin@example.com", "admin_pass_1234").await;
    let service = app.create_service("Paie").await;
    let path = app
        .create_incident(
            "Panne de la paie",
            "La paie ne répond plus.",
            "critical",
            &[service],
        )
        .await;

    let (_, page) = app.get(&path).await;

    assert!(page.contains(
        r##"<a href="#composer" class="btn btn-primary" data-composer-open>Publier une mise à jour</a>"##
    ));
}

#[tokio::test]
async fn a_correction_replaces_the_report() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let (path, id) = resolved_incident(&app).await;
    write_report(&app, id, REPORT).await;

    let (status, _, _) = write_report(&app, id, "Un disque de journaux plein.").await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    let (_, page) = app.get(&path).await;
    assert!(page.contains("Un disque de journaux plein."));
    assert!(!page.contains(REPORT));
}

#[tokio::test]
async fn an_empty_report_comes_back_in_its_open_form() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let (_, id) = resolved_incident(&app).await;

    let (status, body, _) = write_report(&app, id, "   ").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Écrivez le bilan avant de le publier."));
    assert!(body.contains("class=\"review-form is-open\""));
    let (_, list) = app.get("/events").await;
    assert!(!list.contains("· bilan"), "nothing was saved");
}

#[tokio::test]
async fn a_report_waits_for_the_incident_to_be_resolved() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let path = app
        .create_incident("Panne du VPN", "Le VPN ne répond plus.", "minor", &[])
        .await;
    let id = event_id_from_path(&path);
    let (_, page) = app.get(&path).await;
    assert!(!page.contains("Écrire le bilan"));

    let (status, _, _) = write_report(&app, id, REPORT).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn readers_cannot_write_a_report() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let (path, id) = resolved_incident(&app).await;
    app.post_form_with_header_csrf("/logout", &[]).await;
    app.create_user(
        "reader@example.com",
        "reader_pass_1234",
        "Reader",
        Role::Reader,
    )
    .await;
    app.login("reader@example.com", "reader_pass_1234").await;
    let (_, page) = app.get(&path).await;
    assert!(!page.contains("Écrire le bilan"));

    let (status, _, _) = write_report(&app, id, REPORT).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_removed_report_leaves_the_page_and_the_list() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let (path, id) = resolved_incident(&app).await;
    write_report(&app, id, REPORT).await;

    let (status, _, location) = app
        .post_form_with_header_csrf(&format!("/events/{id}/review/delete"), &[])
        .await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location.as_deref(), Some(&*path));
    let (_, page) = app.get(&path).await;
    assert!(!page.contains(REPORT));
    assert!(page.contains("Écrire le bilan"));
    let (_, list) = app.get("/events").await;
    assert!(!list.contains("· bilan"));
}

#[tokio::test]
async fn visitors_read_the_report_without_its_author() {
    let app = TestApp::spawn_public().await;
    app.setup_publisher().await;
    let (path, id) = resolved_incident(&app).await;
    write_report(&app, id, REPORT).await;
    let (_, signed_in) = app.get(&path).await;
    assert!(
        signed_in.contains("·</span> Publisher"),
        "members see who wrote it"
    );

    let (status, _, _) = app.post_form_with_header_csrf("/logout", &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (_, page) = app.get(&path).await;

    assert!(page.contains(REPORT));
    assert!(!page.contains("Publisher"));
}

#[tokio::test]
async fn the_feed_carries_the_report_without_its_author() {
    let app = TestApp::spawn_public().await;
    app.setup_publisher().await;
    let (_, id) = resolved_incident(&app).await;
    write_report(&app, id, REPORT).await;

    let (status, body) = app.get("/feed").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(REPORT));
    assert!(body.contains("&#60;h4&#62;Bilan&#60;/h4&#62;"));
    assert!(!body.contains("Publisher"));
}

/// Undoing a step is shown only to who may do it: an editor on open work,
/// an administrator on anything; a closed incident is the administrators'.
#[tokio::test]
async fn going_back_a_step_is_offered_only_to_who_may_do_it() {
    let app = TestApp::spawn().await;
    app.setup_publisher().await;
    let mut paths = Vec::new();
    for (title, next) in [
        ("Panne du VPN", "in_progress"),
        ("Panne du Wi-Fi", "resolved"),
    ] {
        let path = app
            .create_incident(title, "Ne répond plus.", "minor", &[])
            .await;
        let id = event_id_from_path(&path);
        let (status, _, _) = app
            .post_form_with_header_csrf(
                &format!("/events/{id}/updates"),
                &[("lifecycle", next), ("message", "Suite.")],
            )
            .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        paths.push(path);
    }
    let offers = |body: &str| body.contains("/revert-lifecycle");

    let (_, open) = app.get(&paths[0]).await;
    let (_, closed) = app.get(&paths[1]).await;
    assert!(offers(&open), "an editor undoes a step of open work");
    assert!(!offers(&closed), "a closed incident is the administrators'");

    app.post_form_with_header_csrf("/logout", &[]).await;
    app.create_user(
        "reader@example.com",
        "reader_pass_1234",
        "Reader",
        Role::Reader,
    )
    .await;
    app.login("reader@example.com", "reader_pass_1234").await;
    let (_, open) = app.get(&paths[0]).await;
    assert!(!offers(&open), "a reader cannot undo anything");
}
