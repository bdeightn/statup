//! Folders of services. A folder starts from the Services page; its own
//! page adds and takes out its services, renames it and sets who sees it
//! for the services that inherit it, from one panel, and deletes it at its
//! foot.

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
use crate::repositories::{FolderRepository, NewJournalEntry, ServiceRepository, UserRepository};
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
    name: NameField,
    /// A refused save opens the panel again, with why.
    settings_open: bool,
    i18n: I18n,
}

impl FolderTemplate {
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
    Saved,
    Added(String),
    Removed(String),
}

/// The name field of the panel: the folder's name, or the refused one.
struct NameField {
    value: String,
    error: Option<String>,
}

/// A refused save, shown again in its open panel: what was typed and
/// chosen, and why each part was refused.
struct Posted {
    name: NameField,
    audience: Audience,
    access_error: Option<String>,
}

/// What the page shows besides the folder itself.
#[derive(Default)]
struct Shown {
    notice: Option<FolderNotice>,
    posted: Option<Posted>,
}

#[derive(Deserialize)]
pub struct FolderQuery {
    created: Option<u8>,
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
pub struct FolderSettingsInput {
    #[serde(default)]
    name: String,
    #[serde(default)]
    visibility: String,
    #[serde(default)]
    group_ids: Vec<i64>,
    #[serde(default)]
    member_ids: Vec<i64>,
}

impl FolderSettingsInput {
    fn audience(&self) -> Result<Audience, AppError> {
        Audience::from_form(&self.visibility, &self.group_ids, &self.member_ids, false)
    }
}

/// The posted setting, refused when it would restrict the folder to nobody.
/// A folder already restricted to nobody (its last group removed, its only
/// reader deactivated) keeps the setting, so it can still be renamed.
async fn checked(
    pool: &DbPool,
    input: &FolderSettingsInput,
    before: &Audience,
) -> Result<Audience, AppError> {
    let audience = input.audience()?;
    if before.check(pool, false).await.is_ok() {
        audience.check(pool, false).await?;
    }
    Ok(audience)
}

/// Who sees the folder, as the panel can show it: the members it names who
/// are no longer active readers are left out, so a rename alone never counts
/// as dropping them.
async fn shown_audience(pool: &DbPool, id: i64) -> Result<Audience, AppError> {
    let mut audience = Audience::of_folder(pool, id).await?;
    let readers = UserRepository::active_readers(pool).await?;
    audience
        .member_ids
        .retain(|member| readers.iter().any(|reader| reader.id == *member));
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
    if query.saved.is_some() {
        return Ok(Some(FolderNotice::Saved));
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
    let folders = FolderRepository::names(pool).await?;
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
    let settings_open = shown.posted.is_some();
    let (name, audience, error) = match shown.posted {
        Some(posted) => (posted.name, posted.audience, posted.access_error),
        None => (
            NameField {
                value: folder.name.clone(),
                error: None,
            },
            saved.clone(),
            None,
        ),
    };
    let public_mode = state.is_public_mode();
    let access = AccessBlock::load(&state.pool, &i18n, audience, public_mode, None, error)
        .await?
        .for_viewer(user);
    render(&FolderTemplate {
        frame: Frame::load(&state.pool, Some(user), csrf_token, &i18n).await?,
        summary: summary(&saved, &access, &i18n),
        folder,
        services,
        others,
        access,
        notice: shown.notice,
        name,
        settings_open,
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

/// Saves the folder's name and who sees it, from its one panel; the
/// services that inherit it follow. Both are checked before anything is
/// written, and each change leaves its own line in the journal, none when
/// nothing changed.
pub async fn save_settings(
    RequirePublisher(user): RequirePublisher,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
    HtmlForm(input): HtmlForm<FolderSettingsInput>,
) -> Result<Response, AppError> {
    let folder = find(&state.pool, id).await?;
    let before = shown_audience(&state.pool, id).await?;
    let name = refusal(free_name(&state.pool, &input.name, Some(id)).await, &i18n)?;
    let audience = refusal(checked(&state.pool, &input, &before).await, &i18n)?;
    let (name, audience) = match (name, audience) {
        (Ok(name), Ok(audience)) => (name, audience),
        (name, audience) => {
            let posted = Posted {
                access_error: audience.as_ref().err().cloned(),
                audience: audience.unwrap_or_else(|_| input.audience().unwrap_or(before)),
                name: NameField {
                    value: input.name,
                    error: name.err(),
                },
            };
            let shown = Shown {
                posted: Some(posted),
                ..Shown::default()
            };
            return render_folder(&state, &user, (csrf_token, i18n), id, shown).await;
        }
    };
    FolderRepository::save_settings(&state.pool, id, &name, audience.mode, audience.grants())
        .await?;
    if folder.name != name {
        NewJournalEntry::new(JournalKind::FolderRenamed)
            .by(&user)
            .about(&name)
            .with_detail(&folder.name)
            .record(&state.pool)
            .await;
    }
    let after = shown_audience(&state.pool, id).await?;
    if after != before {
        NewJournalEntry::new(JournalKind::FolderVisibilityChanged)
            .by(&user)
            .about(&name)
            .with_detail(after.detail())
            .record(&state.pool)
            .await;
    }
    Ok(Redirect::to(&format!("/services/folders/{id}?saved=1")).into_response())
}

/// A refused value becomes the sentence that says why; any other error
/// stops the request.
fn refusal<T>(result: Result<T, AppError>, i18n: &I18n) -> Result<Result<T, String>, AppError> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(AppError::Validation(key)) => Ok(Err(i18n.t(&key).to_string())),
        Err(e) => Err(e),
    }
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
    for service in &joined {
        NewJournalEntry::new(JournalKind::FolderServiceAdded)
            .by(&user)
            .about(&service.name)
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
