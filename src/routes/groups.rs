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
use crate::models::{GroupMember, JournalKind, MemberGroup, User, check_group_name};
use crate::repositories::{GroupRepository, NewJournalEntry, UserRepository};
use crate::state::AppState;

#[derive(Template)]
#[template(path = "admin/group.html")]
struct GroupTemplate {
    frame: Frame,
    group: MemberGroup,
    members: Vec<GroupMember>,
    /// Active members not in the group, offered by "Add members".
    outsiders: Vec<GroupMember>,
    /// How many services only this group opens, said before it is deleted.
    closes: usize,
    notice: Option<GroupNotice>,
    rename: RenameForm,
    i18n: I18n,
}

impl GroupTemplate {
    fn closes_label(&self) -> String {
        self.i18n.plural("admin.delete_group_closes", self.closes)
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
            NewJournalEntry::new(JournalKind::GroupCreated)
                .by(&admin)
                .about(&name)
                .record(&state.pool)
                .await;
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
    let group = GroupRepository::find(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
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
    if group.name != name {
        NewJournalEntry::new(JournalKind::GroupRenamed)
            .by(&admin)
            .about(&name)
            .with_detail(&group.name)
            .record(&state.pool)
            .await;
    }
    Ok(Redirect::to(&format!("/admin/groups/{id}?renamed=1")).into_response())
}

pub async fn add_members(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    HtmlForm(input): HtmlForm<AddMembersInput>,
) -> Result<Response, AppError> {
    let group = GroupRepository::find(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    // Only an active member outside the group joins: the others are passed over.
    let joining: Vec<String> = GroupRepository::outsiders(&state.pool, id)
        .await?
        .into_iter()
        .filter(|member| input.user_ids.contains(&member.id))
        .map(|member| member.display_name)
        .collect();
    let added = GroupRepository::add_members(&state.pool, id, &input.user_ids).await?;
    for name in &joining {
        let kind = JournalKind::GroupMemberAdded;
        membership_line(kind, &admin, name, &group)
            .record(&state.pool)
            .await;
    }
    Ok(Redirect::to(&format!("/admin/groups/{id}?added={added}")).into_response())
}

pub async fn remove_member(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path((id, user_id)): Path<(i64, i64)>,
) -> Result<Response, AppError> {
    let group = GroupRepository::find(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let member = UserRepository::find_by_id(&state.pool, user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !GroupRepository::remove_member(&state.pool, id, user_id).await? {
        return Err(AppError::NotFound);
    }
    let kind = JournalKind::GroupMemberRemoved;
    membership_line(kind, &admin, &member.display_name, &group)
        .record(&state.pool)
        .await;
    Ok(Redirect::to(&format!("/admin/groups/{id}?removed={user_id}")).into_response())
}

/// The line for one member who joined or left a group.
fn membership_line(
    kind: JournalKind,
    admin: &User,
    member: &str,
    group: &MemberGroup,
) -> NewJournalEntry {
    NewJournalEntry::new(kind)
        .by(admin)
        .about(member)
        .with_detail(&group.name)
}

/// Its members leave it and the services lose it; a service it alone
/// opened is then seen by editors and administrators only, as its page
/// says before.
pub async fn delete_group(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let group = GroupRepository::find(&state.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !GroupRepository::delete(&state.pool, id).await? {
        return Err(AppError::NotFound);
    }
    NewJournalEntry::new(JournalKind::GroupDeleted)
        .by(&admin)
        .about(&group.name)
        .record(&state.pool)
        .await;
    Ok(Redirect::to("/admin/users?group_deleted=1#groups").into_response())
}
