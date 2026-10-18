//! Folders of services. A folder starts from the Services page; its own
//! page adds and takes out its services, sets who sees it for the services
//! that inherit it, renames it, and deletes it at its foot.

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::audience::{AccessBlock, Audience};
use super::services::new_folder_refused;
use super::{Frame, render};
use crate::db::DbPool;
use crate::error::AppError;
use crate::i18n::{I18n, Locale};
use crate::middleware::{CsrfToken, HtmlForm, RequirePublisher};
use crate::models::{JournalKind, Service, ServiceFolder, User, Visibility, check_folder_name};
use crate::repositories::{FolderRepository, NewJournalEntry, ServiceRepository};
use crate::state::AppState;

/// Past this many, the names in the grey line fold behind "and N more".
const NAMES_SHOWN: usize = 2;

#[derive(Template)]
#[template(path = "services/folder.html")]
struct FolderTemplate {
    frame: Frame,
    folder: ServiceFolder,
    /// Its services, in the chosen order.
    services: Vec<Service>,
    /// The other services, offered by "Add services".
    others: Vec<Candidate>,
    access: AccessBlock,
    /// "Restricted, Accounting": the saved setting in one grey line.
    summary: String,
    notice: Option<FolderNotice>,
    rename: NameForm,
    /// A refused setting opens its panel again, with why.
    visibility_open: bool,
    i18n: I18n,
}

impl FolderTemplate {
    fn visibility_line(&self) -> String {
        self.i18n
            .tf("folders.visibility_line", &[("value", &self.summary)])
    }

    fn remove_label(&self, name: &str) -> String {
        self.i18n.tf("folders.remove_aria", &[("name", name)])
    }
}

/// A service outside the folder, with the folder it would leave.
struct Candidate {
    id: i64,
    name: String,
    folder: Option<String>,
}

/// What the previous action did, said once on top of the page.
enum FolderNotice {
    Created,
    Renamed,
    VisibilitySaved,
    Added(String),
    Removed(String),
}

/// The rename panel, open again with the refused name and why.
#[derive(Default)]
struct NameForm {
    value: Option<String>,
    error: Option<String>,
}

/// What the page shows besides the folder itself.
#[derive(Default)]
struct Shown {
    notice: Option<FolderNotice>,
    rename: NameForm,
    /// A refused setting, shown again with why.
    refused: Option<(Audience, String)>,
}

#[derive(Deserialize)]
pub struct FolderQuery {
    created: Option<u8>,
    renamed: Option<u8>,
    saved: Option<u8>,
    added: Option<usize>,
    kept: Option<usize>,
    removed: Option<i64>,
}

#[derive(Deserialize)]
pub struct FolderNameInput {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
pub struct AddServicesInput {
    #[serde(default)]
    service_ids: Vec<i64>,
}

#[derive(Deserialize)]
pub struct VisibilityInput {
    #[serde(default)]
    visibility: String,
    #[serde(default)]
    group_ids: Vec<i64>,
    #[serde(default)]
    member_ids: Vec<i64>,
}

impl VisibilityInput {
    fn audience(&self) -> Result<Audience, AppError> {
        Audience::from_form(&self.visibility, &self.group_ids, &self.member_ids, false)
    }
}

/// The posted setting, refused when restricted to nobody.
async fn checked(pool: &DbPool, input: &VisibilityInput) -> Result<Audience, AppError> {
    let audience = input.audience()?;
    audience.check(pool, false).await?;
    Ok(audience)
}

/// A name the folder can take: valid and not another folder's, ignoring
/// case, accented letters included.
async fn free_name(pool: &DbPool, raw: &str, except: Option<i64>) -> Result<String, AppError> {
    let name = check_folder_name(raw).map_err(AppError::validation)?;
    if FolderRepository::name_taken(pool, &name, except).await? {
        return Err(AppError::validation("validation.folder_name_taken"));
    }
    Ok(name)
}

async fn find(pool: &DbPool, id: i64) -> Result<ServiceFolder, AppError> {
    FolderRepository::find(pool, id)
        .await?
        .ok_or(AppError::NotFound)
}

async fn notice_of(
    pool: &DbPool,
    query: &FolderQuery,
    i18n: &I18n,
) -> Result<Option<FolderNotice>, AppError> {
    if query.created.is_some() {
        return Ok(Some(FolderNotice::Created));
    }
    if query.renamed.is_some() {
        return Ok(Some(FolderNotice::Renamed));
    }
    if query.saved.is_some() {
        return Ok(Some(FolderNotice::VisibilitySaved));
    }
    if let Some(added) = query.added.filter(|added| *added > 0) {
        let kept = query.kept.unwrap_or(0).min(added);
        return Ok(Some(FolderNotice::Added(added_text(i18n, added, kept))));
    }
    let Some(id) = query.removed else {
        return Ok(None);
    };
    let removed = ServiceRepository::find_by_id(pool, id).await?;
    Ok(removed.map(|service| FolderNotice::Removed(service.name)))
}

/// "3 services added. 2 inherit the folder's visibility, 1 keeps its own."
fn added_text(i18n: &I18n, added: usize, kept: usize) -> String {
    let heirs = added - kept;
    let effect = match (heirs, kept) {
        (_, 0) => i18n.plural("folders.added_all_inherit", added),
        (0, _) => i18n.plural("folders.added_all_keep", added),
        _ => i18n.tf(
            "folders.added_mixed",
            &[
                ("inherit", &i18n.plural("folders.added_inherit", heirs)),
                ("keep", &i18n.plural("folders.added_keep", kept)),
            ],
        ),
    };
    format!("{} {effect}", i18n.plural("folders.added", added))
}

/// The saved setting in a few words: the choice, then the first names it
/// grants.
fn summary(saved: &Audience, access: &AccessBlock, i18n: &I18n) -> String {
    let mode = i18n
        .t(match saved.mode {
            Visibility::Everyone if access.public_mode => "services.visibility_everyone",
            Visibility::Everyone => "services.visibility_everyone_private",
            Visibility::Members => "services.visibility_members",
            Visibility::Groups => "services.visibility_groups",
        })
        .to_string();
    let groups = access
        .groups
        .iter()
        .filter(|group| saved.group_ids.contains(&group.id));
    let members = access
        .members
        .iter()
        .filter(|member| saved.member_ids.contains(&member.id));
    let names: Vec<&str> = groups
        .chain(members)
        .map(|choice| choice.name.as_str())
        .collect();
    if names.is_empty() {
        return mode;
    }
    let shown = names[..names.len().min(NAMES_SHOWN)].join(", ");
    let hidden = names.len().saturating_sub(NAMES_SHOWN);
    if hidden == 0 {
        return format!("{mode}, {shown}");
    }
    let and_more = i18n.tf("services.access_more", &[("n", &hidden.to_string())]);
    format!("{mode}, {shown} {and_more}")
}

/// Its services, and the others with the folder each one is in.
async fn services_around(
    pool: &DbPool,
    id: i64,
) -> Result<(Vec<Service>, Vec<Candidate>), AppError> {
    let folders: HashMap<i64, String> = FolderRepository::list(pool)
        .await?
        .into_iter()
        .map(|folder| (folder.id, folder.name))
        .collect();
    let (inside, outside): (Vec<Service>, Vec<Service>) = ServiceRepository::list_all(pool)
        .await?
        .into_iter()
        .partition(|service| service.folder_id == Some(id));
    let others = outside
        .into_iter()
        .map(|service| Candidate {
            folder: service
                .folder_id
                .and_then(|folder| folders.get(&folder).cloned()),
            id: service.id,
            name: service.name,
        })
        .collect();
    Ok((inside, others))
}

async fn render_folder(
    state: &AppState,
    user: &User,
    (csrf_token, i18n): (String, I18n),
    id: i64,
    shown: Shown,
) -> Result<Response, AppError> {
    let folder = find(&state.pool, id).await?;
    let (services, others) = services_around(&state.pool, id).await?;
    let saved = Audience::of_folder(&state.pool, id).await?;
    let visibility_open = shown.refused.is_some();
    let (audience, error) = match shown.refused {
        Some((posted, error)) => (posted, Some(error)),
        None => (saved.clone(), None),
    };
    let public_mode = state.is_public_mode();
    let access = AccessBlock::load(&state.pool, &i18n, audience, public_mode, None, error).await?;
    render(&FolderTemplate {
        frame: Frame::load(&state.pool, Some(user), csrf_token, &i18n).await?,
        summary: summary(&saved, &access, &i18n),
        folder,
        services,
        others,
        access,
        notice: shown.notice,
        rename: shown.rename,
        visibility_open,
        i18n,
    })
}

pub async fn folder_page(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<FolderQuery>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
) -> Result<Response, AppError> {
    let shown = Shown {
        notice: notice_of(&state.pool, &query, &i18n).await?,
        ..Shown::default()
    };
    render_folder(&state, &user, (csrf_token, i18n), id, shown).await
}

/// A new folder opens on its own page, where its services are added.
pub async fn create_folder(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
    HtmlForm(input): HtmlForm<FolderNameInput>,
) -> Result<Response, AppError> {
    let name = match free_name(&state.pool, &input.name, None).await {
        Ok(name) => name,
        Err(AppError::Validation(key)) => {
            return new_folder_refused(&state, &user, (csrf_token, i18n), &key, input.name).await;
        }
        Err(e) => return Err(e),
    };
    let id = FolderRepository::create(&state.pool, &name).await?;
    NewJournalEntry::new(JournalKind::FolderCreated)
        .by(&user)
        .about(&name)
        .record(&state.pool)
        .await;
    Ok(Redirect::to(&format!("/services/folders/{id}?created=1")).into_response())
}

pub async fn rename_folder(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
    HtmlForm(input): HtmlForm<FolderNameInput>,
) -> Result<Response, AppError> {
    let folder = find(&state.pool, id).await?;
    let name = match free_name(&state.pool, &input.name, Some(id)).await {
        Ok(name) => name,
        Err(AppError::Validation(key)) => {
            let rename = NameForm {
                error: Some(i18n.t(&key).to_string()),
                value: Some(input.name),
            };
            let shown = Shown {
                rename,
                ..Shown::default()
            };
            return render_folder(&state, &user, (csrf_token, i18n), id, shown).await;
        }
        Err(e) => return Err(e),
    };
    FolderRepository::rename(&state.pool, id, &name).await?;
    if folder.name != name {
        NewJournalEntry::new(JournalKind::FolderRenamed)
            .by(&user)
            .about(&name)
            .with_detail(&folder.name)
            .record(&state.pool)
            .await;
    }
    Ok(Redirect::to(&format!("/services/folders/{id}?renamed=1")).into_response())
}

/// Saves who sees the folder; the services that inherit it follow. A save
/// that changes nothing leaves no line in the journal.
pub async fn save_visibility(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
    HtmlForm(input): HtmlForm<VisibilityInput>,
) -> Result<Response, AppError> {
    let folder = find(&state.pool, id).await?;
    let before = Audience::of_folder(&state.pool, id).await?;
    let audience = match checked(&state.pool, &input).await {
        Ok(audience) => audience,
        Err(AppError::Validation(key)) => {
            let posted = input.audience().unwrap_or_else(|_| before.clone());
            let shown = Shown {
                refused: Some((posted, i18n.t(&key).to_string())),
                ..Shown::default()
            };
            return render_folder(&state, &user, (csrf_token, i18n), id, shown).await;
        }
        Err(e) => return Err(e),
    };
    FolderRepository::set_visibility(&state.pool, id, audience.mode, audience.grants()).await?;
    let after = Audience::of_folder(&state.pool, id).await?;
    if after != before {
        NewJournalEntry::new(JournalKind::FolderVisibilityChanged)
            .by(&user)
            .about(&folder.name)
            .with_detail(after.detail())
            .record(&state.pool)
            .await;
    }
    Ok(Redirect::to(&format!("/services/folders/{id}?saved=1")).into_response())
}

/// Puts the ticked services in the folder, out of the one they were in.
/// The page then says how many inherit its setting and how many keep their
/// own.
pub async fn add_services(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    HtmlForm(input): HtmlForm<AddServicesInput>,
) -> Result<Response, AppError> {
    let folder = find(&state.pool, id).await?;
    let joined = FolderRepository::add_services(&state.pool, id, &input.service_ids).await?;
    let names: HashMap<i64, String> = ServiceRepository::list_all(&state.pool)
        .await?
        .into_iter()
        .map(|service| (service.id, service.name))
        .collect();
    for service in &joined {
        let Some(name) = names.get(&service.service_id) else {
            continue;
        };
        NewJournalEntry::new(JournalKind::FolderServiceAdded)
            .by(&user)
            .about(name)
            .with_detail(&folder.name)
            .record(&state.pool)
            .await;
    }
    let added = joined.len();
    let kept = joined.iter().filter(|service| !service.follows).count();
    Ok(Redirect::to(&format!("/services/folders/{id}?added={added}&kept={kept}")).into_response())
}

/// Takes a service out of the folder; whoever saw it still does.
pub async fn remove_service(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    Path((id, service_id)): Path<(i64, i64)>,
) -> Result<Response, AppError> {
    let folder = find(&state.pool, id).await?;
    let service = ServiceRepository::find_by_id(&state.pool, service_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !FolderRepository::remove_service(&state.pool, id, service_id).await? {
        return Err(AppError::NotFound);
    }
    NewJournalEntry::new(JournalKind::FolderServiceRemoved)
        .by(&user)
        .about(&service.name)
        .with_detail(&folder.name)
        .record(&state.pool)
        .await;
    Ok(Redirect::to(&format!("/services/folders/{id}?removed={service_id}")).into_response())
}

#[derive(Deserialize)]
pub struct FolderOrderForm {
    #[serde(default)]
    order: Vec<i64>,
}

/// Saves the order of the folders set by the Services page's script, which
/// reloads the page on a refusal: a folder was added or deleted meanwhile.
pub async fn save_order(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    HtmlForm(form): HtmlForm<FolderOrderForm>,
) -> Result<Response, AppError> {
    if !FolderRepository::reorder(&state.pool, &form.order).await? {
        return Err(AppError::validation("error.invalid_data"));
    }
    NewJournalEntry::new(JournalKind::ServicesReordered)
        .by(&user)
        .record(&state.pool)
        .await;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Its services stay, out of any folder, seen by the same people.
pub async fn delete_folder(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let folder = find(&state.pool, id).await?;
    FolderRepository::delete(&state.pool, id).await?;
    NewJournalEntry::new(JournalKind::FolderDeleted)
        .by(&user)
        .about(&folder.name)
        .record(&state.pool)
        .await;
    let name =
        percent_encoding::utf8_percent_encode(&folder.name, percent_encoding::NON_ALPHANUMERIC);
    Ok(Redirect::to(&format!("/services?folder_deleted={name}")).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notice_after_adding_never_says_zero() {
        let fr = I18n::new("fr");
        assert_eq!(
            added_text(&fr, 3, 1),
            "3 services ajoutés. 2 héritent de la visibilité du dossier, 1 conserve la sienne."
        );
        assert_eq!(
            added_text(&fr, 2, 0),
            "2 services ajoutés. Tous héritent de la visibilité du dossier."
        );
        assert_eq!(
            added_text(&fr, 1, 1),
            "1 service ajouté. Il conserve sa propre visibilité."
        );
    }
}
