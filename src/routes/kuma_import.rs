//! Importing the monitors of an Uptime Kuma status page, from "New" on the
//! Services page: the page is read and its groups ticked, then it is read
//! again and imported, so nothing the form carries but the URL and the
//! ticked names decides what is created.

use askama::Template;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::checks::failure_key;
use super::{Frame, render};
use crate::error::AppError;
use crate::i18n::{I18n, Locale};
use crate::middleware::{CsrfToken, HtmlForm, RequirePublisher};
use crate::models::User;
use crate::repositories::{FolderRepository, ServiceRepository};
use crate::services::{
    CHECK_TIMEOUT, FolderPlan, GroupPlan, KumaError, KumaPage, LastImport, MonitorFate,
    MonitorPlan, import_groups, kuma_client, plan_import, read_status_page, status_page_api,
};
use crate::state::AppState;

#[derive(Template)]
#[template(path = "services/import.html")]
struct ImportTemplate {
    frame: Frame,
    address: String,
    internal_cert: bool,
    /// Why the page could not be read, under its URL.
    error: Option<String>,
    /// The page once read.
    read: Option<ReadPage>,
    /// Nothing was ticked: said over the groups.
    nothing_ticked: bool,
    i18n: I18n,
}

impl ImportTemplate {
    /// What importing a group does, in a few facts.
    fn group_facts(&self, group: &GroupPlan) -> Vec<String> {
        if group.monitors.is_empty() {
            return vec![self.i18n.t("kuma_import.no_monitor").to_string()];
        }
        let count = |kept: fn(&MonitorFate) -> bool| {
            group
                .monitors
                .iter()
                .filter(|monitor| kept(&monitor.fate))
                .count()
        };
        let counts = [
            ("kuma_import.to_import", group.new_services()),
            (
                "kuma_import.existing",
                count(|fate| *fate == MonitorFate::Exists),
            ),
            (
                "kuma_import.refused",
                count(|fate| matches!(fate, MonitorFate::Refused(_))),
            ),
        ];
        let mut facts: Vec<String> = counts
            .into_iter()
            .filter(|(_, n)| *n > 0)
            .map(|(key, n)| self.i18n.plural(key, n))
            .collect();
        if group.new_services() > 0 && matches!(group.folder, FolderPlan::Existing { .. }) {
            facts.push(self.i18n.t("kuma_import.folder_existing").to_string());
        }
        facts
    }

    /// Why a monitor is left, for screen readers: its tag only looks quiet.
    fn kept_reason(&self, monitor: &MonitorPlan) -> &'static str {
        match monitor.fate {
            MonitorFate::Refused(key) => self.i18n.t(key),
            _ => self.i18n.t("kuma_import.existing_sr"),
        }
    }
}

/// A page read, with what importing each group would do.
struct ReadPage {
    title: Option<String>,
    groups: Vec<GroupPlan>,
}

impl ReadPage {
    fn importable(&self) -> bool {
        self.groups.iter().any(|group| group.new_services() > 0)
    }
}

#[derive(Deserialize)]
pub struct ImportInput {
    #[serde(default)]
    address: String,
    #[serde(default)]
    internal_cert: Option<String>,
    #[serde(default)]
    groups: Vec<String>,
}

impl ImportInput {
    fn last(&self) -> LastImport {
        LastImport {
            address: self.address.trim().to_string(),
            internal_cert: self.internal_cert.is_some(),
        }
    }
}

/// The form, filled with the page imported last.
pub async fn page(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
) -> Result<Response, AppError> {
    let last = LastImport::load(&state.pool).await?;
    let shown = Shown {
        last,
        ..Shown::default()
    };
    render_import(&state, &user, (csrf_token, i18n), shown).await
}

/// Reads the page and shows its groups, ready to tick.
pub async fn read(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
    HtmlForm(input): HtmlForm<ImportInput>,
) -> Result<Response, AppError> {
    let shown = match read_page(&input, &i18n).await? {
        Ok(page) => Shown {
            page: Some(page),
            ..Shown::of(&input)
        },
        Err(error) => Shown {
            error: Some(error),
            ..Shown::of(&input)
        },
    };
    render_import(&state, &user, (csrf_token, i18n), shown).await
}

/// Reads the page again and imports the ticked groups; the Services page
/// then says what was created.
pub async fn import(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
    HtmlForm(input): HtmlForm<ImportInput>,
) -> Result<Response, AppError> {
    let page = match read_page(&input, &i18n).await? {
        Ok(page) => page,
        Err(error) => {
            let shown = Shown {
                error: Some(error),
                ..Shown::of(&input)
            };
            return render_import(&state, &user, (csrf_token, i18n), shown).await;
        }
    };
    if !input.groups.is_empty() {
        let imported = import_groups(&state.pool, &page, &input.groups, &user).await?;
        if imported.services > 0 {
            input.last().save(&state.pool).await?;
            let to = format!(
                "/services?imported={}&folders={}",
                imported.services, imported.folders
            );
            return Ok(Redirect::to(&to).into_response());
        }
    }
    // Nothing ticked, or nothing left to create since the page was read.
    let shown = Shown {
        page: Some(page),
        nothing_ticked: input.groups.is_empty(),
        ..Shown::of(&input)
    };
    render_import(&state, &user, (csrf_token, i18n), shown).await
}

/// What the page shows besides the frame.
#[derive(Default)]
struct Shown {
    last: LastImport,
    error: Option<String>,
    page: Option<KumaPage>,
    nothing_ticked: bool,
}

impl Shown {
    fn of(input: &ImportInput) -> Self {
        Self {
            last: input.last(),
            ..Self::default()
        }
    }
}

async fn render_import(
    state: &AppState,
    user: &User,
    (csrf_token, i18n): (String, I18n),
    shown: Shown,
) -> Result<Response, AppError> {
    let read = match shown.page {
        Some(page) => {
            let services = ServiceRepository::list_all(&state.pool).await?;
            let folders = FolderRepository::list(&state.pool).await?;
            Some(ReadPage {
                groups: plan_import(&page, &services, &folders),
                title: page.title,
            })
        }
        None => None,
    };
    render(&ImportTemplate {
        frame: Frame::load(&state.pool, Some(user), csrf_token, &i18n).await?,
        address: shown.last.address,
        internal_cert: shown.last.internal_cert,
        error: shown.error,
        read,
        nothing_ticked: shown.nothing_ticked,
        i18n,
    })
}

/// The page the form names, or why it could not be read, in the reader's
/// words.
async fn read_page(input: &ImportInput, i18n: &I18n) -> Result<Result<KumaPage, String>, AppError> {
    let api = match status_page_api(&input.address) {
        Ok(api) => api,
        Err(key) => return Ok(Err(i18n.t(key).to_string())),
    };
    let client =
        kuma_client(input.internal_cert.is_some(), CHECK_TIMEOUT).map_err(anyhow::Error::from)?;
    Ok(read_status_page(&client, &api)
        .await
        .map_err(|error| error_text(error, i18n)))
}

fn error_text(error: KumaError, i18n: &I18n) -> String {
    match error {
        KumaError::NotFound => i18n.t("kuma_import.not_found").to_string(),
        KumaError::Status(status) => i18n.tf(
            "kuma_import.error_status",
            &[("status", &status.to_string())],
        ),
        KumaError::TooLarge => i18n.t("kuma_import.too_large").to_string(),
        KumaError::NotKuma => i18n.t("kuma_import.not_kuma").to_string(),
        KumaError::Network(finding) => i18n.t(failure_key(finding)).to_string(),
    }
}
