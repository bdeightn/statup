//! Groups of members. The Team page lists them; each group has its own
//! page, where members are added and removed at once, the group renamed,
//! and deleted at its foot. A service then says which groups see it.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::admin::refused_page;
use super::{Frame, render};
use crate::error::AppError;
use crate::i18n::{I18n, Locale};
use crate::middleware::{CsrfToken, HtmlForm, RequireAdmin};
use crate::models::{GroupMember, MemberGroup, User, check_group_name};
use crate::repositories::{GroupRepository, UserRepository};
use crate::state::AppState;

#[derive(Template)]
#[template(path = "admin/group.html")]
struct GroupTemplate {
    frame: Frame,
    group: MemberGroup,
    members: Vec<GroupMember>,
    /// Active members not in the group, offered by "Add members".
    outsiders: Vec<GroupMember>,
    /// Services only this group opens, named before it is deleted.
    closes: Vec<String>,
    notice: Option<GroupNotice>,
    rename: RenameForm,
    i18n: I18n,
}

impl GroupTemplate {
    fn closes_label(&self) -> String {
        self.closes.join(", ")
    }
}

/// What the previous action did, said once on top of the page.
enum GroupNotice {
    Created,
    Renamed,
    Added(String),
    Removed(String),
}

/// The rename panel, open again with the refused name and why.
#[derive(Default)]
struct RenameForm {
    value: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
pub struct GroupQuery {
    created: Option<u8>,
    renamed: Option<u8>,
    added: Option<usize>,
    removed: Option<i64>,
}

#[derive(Deserialize)]
pub struct GroupNameInput {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
pub struct AddMembersInput {
    #[serde(default)]
    user_ids: Vec<i64>,
}

/// A name the group can take: valid and not another group's, ignoring case.
async fn free_name(state: &AppState, raw: &str, except: Option<i64>) -> Result<String, AppError> {
    let name = check_group_name(raw).map_err(AppError::validation)?;
    if GroupRepository::name_taken(&state.pool, &name, except).await? {
        return Err(AppError::validation("validation.group_name_taken"));
    }
    Ok(name)
}

async fn notice_of(
    state: &AppState,
    query: &GroupQuery,
    i18n: &I18n,
) -> Result<Option<GroupNotice>, AppError> {
    if query.created.is_some() {
        return Ok(Some(GroupNotice::Created));
    }
    if query.renamed.is_some() {
        return Ok(Some(GroupNotice::Renamed));
    }
    if let Some(count) = query.added.filter(|count| *count > 0) {
        let text = i18n.plural("admin.notice_members_added", count);
        return Ok(Some(GroupNotice::Added(text)));
    }
    let Some(id) = query.removed else {
        return Ok(None);
    };
    let removed = UserRepository::find_by_id(&state.pool, id).await?;
    Ok(removed.map(|user| GroupNotice::Removed(user.display_name)))
}

async fn render_group(
    state: &AppState,
    admin: &User,
    csrf_token: String,
    i18n: I18n,
    id: i64,
    (notice, rename): (Option<GroupNotice>, RenameForm),
) -> Result<Response, AppError> {
    let group = GroupRepository::find(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    render(&GroupTemplate {
        frame: Frame::load(&state.pool, Some(admin), csrf_token, &i18n).await?,
        members: GroupRepository::members(&state.pool, id).await?,
        outsiders: GroupRepository::outsiders(&state.pool, id).await?,
        closes: GroupRepository::services_closed_by_deleting(&state.pool, id).await?,
        group,
        notice,
        rename,
        i18n,
    })
}

pub async fn group_page(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<GroupQuery>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
) -> Result<Response, AppError> {
    let notice = notice_of(&state, &query, &i18n).await?;
    let page = (notice, RenameForm::default());
    render_group(&state, &admin, csrf_token, i18n, id, page).await
}

/// A new group opens on its own page, where its members are added.
pub async fn create_group(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
    HtmlForm(input): HtmlForm<GroupNameInput>,
) -> Result<Response, AppError> {
    match free_name(&state, &input.name, None).await {
        Ok(name) => {
            let id = GroupRepository::create(&state.pool, &name).await?;
            Ok(Redirect::to(&format!("/admin/groups/{id}?created=1")).into_response())
        }
        Err(AppError::Validation(key)) => {
            refused_page(&state, &admin, csrf_token, i18n, &key, input.name).await
        }
        Err(e) => Err(e),
    }
}

pub async fn rename_group(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    CsrfToken(csrf_token): CsrfToken,
    Locale(i18n): Locale,
    HtmlForm(input): HtmlForm<GroupNameInput>,
) -> Result<Response, AppError> {
    let name = match free_name(&state, &input.name, Some(id)).await {
        Ok(name) => name,
        Err(AppError::Validation(key)) => {
            let rename = RenameForm {
                error: Some(i18n.t(&key).to_string()),
                value: Some(input.name),
            };
            return render_group(&state, &admin, csrf_token, i18n, id, (None, rename)).await;
        }
        Err(e) => return Err(e),
    };
    if !GroupRepository::rename(&state.pool, id, &name).await? {
        return Err(AppError::NotFound);
    }
    Ok(Redirect::to(&format!("/admin/groups/{id}?renamed=1")).into_response())
}

pub async fn add_members(
    RequireAdmin(_admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    HtmlForm(input): HtmlForm<AddMembersInput>,
) -> Result<Response, AppError> {
    if GroupRepository::find(&state.pool, id).await?.is_none() {
        return Err(AppError::NotFound);
    }
    let added = GroupRepository::add_members(&state.pool, id, &input.user_ids).await?;
    Ok(Redirect::to(&format!("/admin/groups/{id}?added={added}")).into_response())
}

pub async fn remove_member(
    RequireAdmin(_admin): RequireAdmin,
    State(state): State<AppState>,
    Path((id, user_id)): Path<(i64, i64)>,
) -> Result<Response, AppError> {
    if !GroupRepository::remove_member(&state.pool, id, user_id).await? {
        return Err(AppError::NotFound);
    }
    Ok(Redirect::to(&format!("/admin/groups/{id}?removed={user_id}")).into_response())
}

/// Its members leave it and the services lose it; a service it alone
/// opened is then seen by editors and administrators only, as its page
/// says before.
pub async fn delete_group(
    RequireAdmin(_admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    if !GroupRepository::delete(&state.pool, id).await? {
        return Err(AppError::NotFound);
    }
    Ok(Redirect::to("/admin/users?group_deleted=1#groups").into_response())
}
