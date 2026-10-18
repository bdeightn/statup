//! Groups of members, on the Team page: create, rename and delete a group,
//! and choose the groups of a member. A service then says which groups see
//! it.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::admin::refused_page;
use crate::error::AppError;
use crate::i18n::Locale;
use crate::middleware::{CsrfToken, HtmlForm, RequireAdmin};
use crate::models::check_group_name;
use crate::repositories::{GroupRepository, UserRepository};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct GroupNameInput {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
pub struct MemberGroupsInput {
    #[serde(default)]
    group_ids: Vec<i64>,
}

/// A name the group can take: valid and not another group's, ignoring case.
async fn free_name(state: &AppState, raw: &str, except: Option<i64>) -> Result<String, AppError> {
    let name = check_group_name(raw).map_err(AppError::validation)?;
    if GroupRepository::name_taken(&state.pool, &name, except).await? {
        return Err(AppError::validation("validation.group_name_taken"));
    }
    Ok(name)
}

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
            Ok(Redirect::to(&format!("/admin/users?group_created={id}#groups")).into_response())
        }
        Err(AppError::Validation(key)) => {
            refused_page(&state, &admin, csrf_token, i18n, &key, Some(input.name)).await
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
            return refused_page(&state, &admin, csrf_token, i18n, &key, None).await;
        }
        Err(e) => return Err(e),
    };
    if !GroupRepository::rename(&state.pool, id, &name).await? {
        return Err(AppError::NotFound);
    }
    Ok(Redirect::to(&format!("/admin/users?group_renamed={id}#groups")).into_response())
}

/// Its members leave it and the services lose it; a service it alone
/// opened is then seen by editors and administrators only, as the
/// confirmation says.
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

pub async fn set_member_groups(
    RequireAdmin(_admin): RequireAdmin,
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    HtmlForm(input): HtmlForm<MemberGroupsInput>,
) -> Result<Response, AppError> {
    if UserRepository::find_by_id(&state.pool, user_id)
        .await?
        .is_none()
    {
        return Err(AppError::NotFound);
    }
    GroupRepository::set_member_groups(&state.pool, user_id, &input.group_ids).await?;
    Ok(Redirect::to(&format!(
        "/admin/users?groups_of={user_id}#member-{user_id}"
    ))
    .into_response())
}
