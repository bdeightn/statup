//! What a group opens, from its own page: the restricted folders and
//! services it may see, added from a panel and taken away one by one.
//! Adding only ever opens: what is public or open to members is seen
//! already, so nothing is offered that would hide a service from others.

use std::collections::{HashMap, HashSet};

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::audience::Audience;
use crate::db::DbPool;
use crate::error::AppError;
use crate::i18n::I18n;
use crate::middleware::{HtmlForm, RequireAdmin};
use crate::models::{JournalKind, Service, ServiceFolder, User, Visibility};
use crate::repositories::{FolderRepository, GroupRepository, NewJournalEntry, ServiceRepository};
use crate::state::AppState;

/// A restricted folder, with the services that follow it.
pub struct AccessFolder {
    pub id: i64,
    pub name: String,
    pub followers: usize,
}

/// A restricted service with its own setting, and the folder it is in.
pub struct AccessService {
    pub service: Service,
    pub folder: Option<String>,
}

/// What the group opens, and what may still be opened to it.
pub struct GroupAccess {
    pub folders: Vec<AccessFolder>,
    pub services: Vec<AccessService>,
    pub openable_folders: Vec<AccessFolder>,
    pub openable_services: Vec<AccessService>,
}

impl GroupAccess {
    pub async fn load(pool: &DbPool, group_id: i64) -> Result<Self, AppError> {
        let granted_folders: HashSet<i64> = GroupRepository::folder_ids(pool, group_id)
            .await?
            .into_iter()
            .collect();
        let granted_services: HashSet<i64> = GroupRepository::own_service_ids(pool, group_id)
            .await?
            .into_iter()
            .collect();
        let services = ServiceRepository::list_all(pool).await?;
        let folders = FolderRepository::list(pool).await?;
        let names: HashMap<i64, String> = folders
            .iter()
            .map(|folder| (folder.id, folder.name.clone()))
            .collect();
        let (folders, openable_folders) = restricted_folders(folders, &services)
            .into_iter()
            .partition(|folder| granted_folders.contains(&folder.id));
        let (services, openable_services) = restricted_services(services, &names)
            .into_iter()
            .partition(|item| granted_services.contains(&item.service.id));
        Ok(Self {
            folders,
            services,
            openable_folders,
            openable_services,
        })
    }

    /// The services the group sees through what it opens.
    pub fn reached(&self) -> usize {
        self.folders.iter().map(|f| f.followers).sum::<usize>() + self.services.len()
    }

    pub fn is_empty(&self) -> bool {
        self.folders.is_empty() && self.services.is_empty()
    }

    pub fn nothing_to_open(&self) -> bool {
        self.openable_folders.is_empty() && self.openable_services.is_empty()
    }

    /// The panel gets a search past eight choices.
    pub fn searchable(&self) -> bool {
        self.openable_folders.len() + self.openable_services.len() > crate::models::SEARCH_FROM
    }
}

fn restricted_folders(folders: Vec<ServiceFolder>, services: &[Service]) -> Vec<AccessFolder> {
    folders
        .into_iter()
        .filter(|folder| folder.visibility == Visibility::Groups)
        .map(|folder| AccessFolder {
            followers: services
                .iter()
                .filter(|s| s.folder_id == Some(folder.id) && s.follows_folder)
                .count(),
            id: folder.id,
            name: folder.name,
        })
        .collect()
}

/// A service that follows its folder is reached through the folder.
fn restricted_services(
    services: Vec<Service>,
    folders: &HashMap<i64, String>,
) -> Vec<AccessService> {
    services
        .into_iter()
        .filter(|s| s.visibility == Visibility::Groups && !s.follows_folder)
        .map(|service| AccessService {
            folder: service.folder_id.and_then(|id| folders.get(&id).cloned()),
            service,
        })
        .collect()
}

/// The sentence after opening or taking away, said once on the page.
pub async fn notice(
    pool: &DbPool,
    i18n: &I18n,
    query: &AccessQuery,
) -> Result<Option<String>, AppError> {
    if let Some(opened) = query.opened.filter(|n| *n > 0) {
        return Ok(Some(i18n.plural("admin.notice_opened", opened)));
    }
    let name = match (query.closed_folder, query.closed_service) {
        (Some(id), _) => FolderRepository::find(pool, id).await?.map(|f| f.name),
        (None, Some(id)) => ServiceRepository::find_by_id(pool, id)
            .await?
            .map(|s| s.name),
        (None, None) => None,
    };
    let Some(name) = name else {
        return Ok(None);
    };
    let removed = i18n.tf("admin.notice_access_removed", &[("name", &name)]);
    Ok(Some(match query.staff_only.filter(|n| *n > 0) {
        Some(n) => format!("{removed} {}", i18n.plural("admin.notice_staff_only", n)),
        None => removed,
    }))
}

/// The page's query after an action on what the group opens.
#[derive(Debug, Default, Deserialize)]
pub struct AccessQuery {
    opened: Option<usize>,
    closed_folder: Option<i64>,
    closed_service: Option<i64>,
    staff_only: Option<usize>,
}

#[derive(Deserialize)]
pub struct OpenInput {
    #[serde(default)]
    folder_ids: Vec<i64>,
    #[serde(default)]
    service_ids: Vec<i64>,
}

async fn find_group(pool: &DbPool, id: i64) -> Result<(), AppError> {
    GroupRepository::find(pool, id)
        .await?
        .map(|_| ())
        .ok_or(AppError::NotFound)
}

/// Opens the ticked folders and services to the group, each one written in
/// the journal as its own page writes it.
pub async fn open(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    HtmlForm(input): HtmlForm<OpenInput>,
) -> Result<Response, AppError> {
    find_group(&state.pool, id).await?;
    let (folders, services) =
        GroupRepository::open_to(&state.pool, id, &input.folder_ids, &input.service_ids).await?;
    for folder in &folders {
        record_folder(&state.pool, &admin, *folder).await;
    }
    for service in &services {
        record_service(&state.pool, &admin, *service).await;
    }
    let mut opened = services.len();
    for folder in &folders {
        opened += FolderRepository::follower_count(&state.pool, *folder).await?;
    }
    Ok(Redirect::to(&format!("/admin/groups/{id}?opened={opened}")).into_response())
}

pub async fn close_folder(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path((id, folder_id)): Path<(i64, i64)>,
) -> Result<Response, AppError> {
    find_group(&state.pool, id).await?;
    if !GroupRepository::close_folder(&state.pool, id, folder_id).await? {
        return Err(AppError::NotFound);
    }
    let staff_only = match record_folder(&state.pool, &admin, folder_id).await {
        Some(audience) if opens_to_nobody(&audience) => {
            FolderRepository::follower_count(&state.pool, folder_id).await?
        }
        _ => 0,
    };
    let to = format!("/admin/groups/{id}?closed_folder={folder_id}&staff_only={staff_only}");
    Ok(Redirect::to(&to).into_response())
}

pub async fn close_service(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path((id, service_id)): Path<(i64, i64)>,
) -> Result<Response, AppError> {
    find_group(&state.pool, id).await?;
    if !GroupRepository::close_service(&state.pool, id, service_id).await? {
        return Err(AppError::NotFound);
    }
    let audience = record_service(&state.pool, &admin, service_id).await;
    let staff_only = usize::from(audience.as_ref().is_some_and(opens_to_nobody));
    let to = format!("/admin/groups/{id}?closed_service={service_id}&staff_only={staff_only}");
    Ok(Redirect::to(&to).into_response())
}

/// Restricted, with no group and no member left: only the team sees it.
fn opens_to_nobody(audience: &Audience) -> bool {
    audience.mode == Visibility::Groups
        && audience.group_ids.is_empty()
        && audience.member_ids.is_empty()
}

/// A line that cannot be written never fails the change it follows. Returns
/// who sees the folder now, when it could be read.
async fn record_folder(pool: &DbPool, admin: &User, id: i64) -> Option<Audience> {
    let read = async {
        let folder = FolderRepository::find(pool, id)
            .await?
            .ok_or(AppError::NotFound)?;
        Ok::<_, AppError>((folder.name, Audience::of_folder(pool, id).await?))
    };
    match read.await {
        Ok((name, audience)) => {
            NewJournalEntry::new(JournalKind::FolderVisibilityChanged)
                .by(admin)
                .about(&name)
                .with_detail(audience.detail())
                .record(pool)
                .await;
            Some(audience)
        }
        Err(error) => {
            tracing::warn!(%error, "Audience of a folder changed from a group not read");
            None
        }
    }
}

async fn record_service(pool: &DbPool, admin: &User, id: i64) -> Option<Audience> {
    let read = async {
        let service = ServiceRepository::find_by_id(pool, id)
            .await?
            .ok_or(AppError::NotFound)?;
        Ok::<_, AppError>((service.name, Audience::of_service(pool, id).await?))
    };
    match read.await {
        Ok((name, audience)) => {
            NewJournalEntry::new(JournalKind::ServiceVisibilityChanged)
                .by(admin)
                .about(&name)
                .with_detail(audience.detail())
                .record(pool)
                .await;
            Some(audience)
        }
        Err(error) => {
            tracing::warn!(%error, "Audience of a service changed from a group not read");
            None
        }
    }
}
