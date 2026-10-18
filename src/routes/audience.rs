//! Who sees a service or a folder: the fields of its form, the checks made
//! before saving, the journal detail and the "Visibility" block both pages
//! draw.

use std::collections::HashMap;

use crate::db::DbPool;
use crate::error::AppError;
use crate::i18n::I18n;
use crate::models::{Grants, Visibility};
use crate::repositories::{FolderRepository, GroupRepository, ServiceRepository, UserRepository};

pub const VISIBILITY_REQUIRED: &str = "validation.visibility_required";
pub const VISIBILITY_CHOICE: &str = "validation.visibility_choice";
pub const VISIBILITY_NO_FOLDER: &str = "validation.visibility_no_folder";

/// Past this many, the granted groups and members fold behind "and N more".
const TAGS_SHOWN: usize = 5;

/// Who sees a service or a folder, as its form holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audience {
    pub mode: Visibility,
    pub group_ids: Vec<i64>,
    pub member_ids: Vec<i64>,
    /// A service in a folder takes the folder's setting.
    pub inherit: bool,
}

impl Audience {
    pub const EVERYONE: Self = Self {
        mode: Visibility::Everyone,
        group_ids: Vec::new(),
        member_ids: Vec::new(),
        inherit: false,
    };

    /// The posted fields. With "inherit" ticked the other fields are
    /// disabled and not sent; otherwise the choice must be one of the
    /// three, never a default that would open the service.
    pub fn from_form(
        visibility: &str,
        group_ids: &[i64],
        member_ids: &[i64],
        inherit: bool,
    ) -> Result<Self, AppError> {
        if inherit {
            return Ok(Self {
                inherit: true,
                ..Self::EVERYONE
            });
        }
        let mode = visibility
            .parse::<Visibility>()
            .map_err(|_| AppError::validation(VISIBILITY_CHOICE))?;
        let restricted = mode == Visibility::Groups;
        Ok(Self {
            mode,
            group_ids: if restricted {
                group_ids.to_vec()
            } else {
                Vec::new()
            },
            member_ids: if restricted {
                member_ids.to_vec()
            } else {
                Vec::new()
            },
            inherit: false,
        })
    }

    pub fn grants(&self) -> Grants<'_> {
        Grants {
            groups: &self.group_ids,
            members: &self.member_ids,
        }
    }

    /// Who sees a service, as saved.
    pub async fn of_service(pool: &DbPool, id: i64) -> Result<Self, AppError> {
        let service = ServiceRepository::find_by_id(pool, id)
            .await?
            .ok_or(AppError::NotFound)?;
        Ok(Self {
            mode: service.visibility,
            group_ids: ServiceRepository::group_ids_of(pool, id).await?,
            member_ids: ServiceRepository::member_ids_of(pool, id).await?,
            inherit: service.follows_folder,
        })
    }

    /// Who sees a folder, as saved.
    pub async fn of_folder(pool: &DbPool, id: i64) -> Result<Self, AppError> {
        let folder = FolderRepository::find(pool, id)
            .await?
            .ok_or(AppError::NotFound)?;
        Ok(Self {
            mode: folder.visibility,
            group_ids: FolderRepository::group_ids_of(pool, id).await?,
            member_ids: FolderRepository::member_ids_of(pool, id).await?,
            inherit: false,
        })
    }

    /// The journal gives counts, never names: a service can be open to
    /// dozens of groups and members.
    pub fn detail(&self) -> String {
        if self.inherit {
            return "folder".to_string();
        }
        match self.mode {
            Visibility::Groups => format!(
                "groups:{};members:{}",
                self.group_ids.len(),
                self.member_ids.len()
            ),
            other => other.as_str().to_string(),
        }
    }

    /// A restricted choice needs a group that exists or an active reader:
    /// otherwise only the team would see it, without a word. Inheriting
    /// needs a folder, which may have gone while the form was open.
    pub async fn check(&self, pool: &DbPool, in_folder: bool) -> Result<(), AppError> {
        if self.inherit {
            return if in_folder {
                Ok(())
            } else {
                Err(AppError::validation(VISIBILITY_NO_FOLDER))
            };
        }
        if self.mode == Visibility::Groups
            && UserRepository::readers_reached(pool, self.grants()).await? == 0
            && GroupRepository::count_of(pool, &self.group_ids).await? == 0
        {
            return Err(AppError::validation(VISIBILITY_REQUIRED));
        }
        Ok(())
    }

    /// Saves who sees the service: its folder's setting, or its own.
    pub async fn save_for_service(
        &self,
        pool: &DbPool,
        id: i64,
        folder_id: Option<i64>,
    ) -> Result<(), AppError> {
        match (self.inherit, folder_id) {
            (true, Some(folder_id)) => {
                FolderRepository::follow(pool, folder_id, id).await?;
            }
            _ => ServiceRepository::set_visibility(pool, id, self.mode, self.grants()).await?,
        }
        Ok(())
    }
}

/// A group or a member offered in the block, granted or not.
pub struct GrantChoice {
    pub id: i64,
    pub name: String,
    /// "2 members" for a group, the address for a member.
    pub meta: String,
    /// The readers of a group; none for a member.
    pub size: Option<usize>,
    /// The readers a group reaches, for the sentence redrawn in the page.
    pub readers: String,
    pub granted: bool,
}

impl GrantChoice {
    /// A group with no reader opens the service to nobody.
    pub fn is_empty_group(&self) -> bool {
        self.size == Some(0)
    }
}

/// A group or member drawn as a tag, hidden until granted.
pub struct GrantTag<'a> {
    pub field: &'static str,
    pub choice: &'a GrantChoice,
    pub is_group: bool,
    /// Granted, but folded behind "and N more" until asked for.
    pub extra: bool,
}

/// The "Visibility" block of a service form or of a folder.
pub struct AccessBlock {
    pub audience: Audience,
    /// "Members" only differs from "Public" on an open page.
    pub public_mode: bool,
    /// "Inherit from folder X", for a service in a folder.
    pub inherit_label: Option<String>,
    pub groups: Vec<GrantChoice>,
    pub members: Vec<GrantChoice>,
    pub effect: String,
    /// "and N more" past the first tags.
    pub more_label: Option<String>,
    pub error: Option<String>,
}

impl AccessBlock {
    pub async fn load(
        pool: &DbPool,
        i18n: &I18n,
        audience: Audience,
        public_mode: bool,
        folder: Option<&str>,
        error: Option<String>,
    ) -> Result<Self, AppError> {
        let groups = group_choices(pool, i18n, &audience).await?;
        let members: Vec<GrantChoice> = UserRepository::active_readers(pool)
            .await?
            .into_iter()
            .map(|reader| GrantChoice {
                granted: audience.member_ids.contains(&reader.id),
                id: reader.id,
                name: reader.display_name,
                readers: reader.id.to_string(),
                meta: reader.email,
                size: None,
            })
            .collect();
        let granted = groups.iter().chain(&members).filter(|c| c.granted).count();
        let hidden_count = granted.saturating_sub(TAGS_SHOWN).to_string();
        let reached = UserRepository::readers_reached(pool, audience.grants()).await?;
        Ok(Self {
            effect: effect(
                i18n,
                &audience,
                public_mode,
                usize::try_from(reached).unwrap_or(0),
            ),
            inherit_label: folder
                .map(|name| i18n.tf("services.visibility_inherit", &[("folder", name)])),
            more_label: (granted > TAGS_SHOWN)
                .then(|| i18n.tf("services.access_more", &[("n", hidden_count.as_str())])),
            audience,
            public_mode,
            groups,
            members,
            error,
        })
    }

    pub fn is(&self, mode: &str) -> bool {
        self.audience.mode.as_str() == mode
    }

    /// "Members" shows on an open page, or when already chosen.
    pub fn offers_members(&self) -> bool {
        self.public_mode || self.audience.mode == Visibility::Members
    }

    /// Every group then member as a tag, shown when granted: the page's
    /// script shows and hides them as they are added and taken off. Past
    /// the first five granted, they fold behind "and N more".
    pub fn tags(&self) -> Vec<GrantTag<'_>> {
        let groups = self.groups.iter().map(|g| ("group_ids", g, true));
        let members = self.members.iter().map(|m| ("member_ids", m, false));
        let mut granted = 0;
        groups
            .chain(members)
            .map(|(field, choice, is_group)| {
                if choice.granted {
                    granted += 1;
                }
                GrantTag {
                    field,
                    choice,
                    is_group,
                    extra: choice.granted && granted > TAGS_SHOWN,
                }
            })
            .collect()
    }

    /// The panel offers what is not granted yet; a search past eight.
    pub fn searchable(&self) -> bool {
        self.groups.len() + self.members.len() > crate::models::SEARCH_FROM
    }
}

async fn group_choices(
    pool: &DbPool,
    i18n: &I18n,
    audience: &Audience,
) -> Result<Vec<GrantChoice>, AppError> {
    let mut readers: HashMap<i64, Vec<i64>> = HashMap::new();
    for (group, reader) in GroupRepository::reader_ids(pool).await? {
        readers.entry(group).or_default().push(reader);
    }
    let choices = GroupRepository::list(pool)
        .await?
        .into_iter()
        .map(|group| {
            let ids = readers.remove(&group.id).unwrap_or_default();
            GrantChoice {
                granted: audience.group_ids.contains(&group.id),
                meta: if ids.is_empty() {
                    i18n.t("services.access_group_empty").to_string()
                } else {
                    i18n.plural("services.access_group_size", ids.len())
                },
                size: Some(ids.len()),
                readers: ids.iter().map(i64::to_string).collect::<Vec<_>>().join(","),
                id: group.id,
                name: group.name,
            }
        })
        .collect();
    Ok(choices)
}

/// The sentence under the block: who will see the service.
fn effect(i18n: &I18n, audience: &Audience, public_mode: bool, reached: usize) -> String {
    match audience.mode {
        Visibility::Everyone if public_mode => i18n.t("services.effect_public").to_string(),
        Visibility::Everyone => i18n.t("services.effect_all_members").to_string(),
        Visibility::Members => i18n.t("services.effect_members").to_string(),
        Visibility::Groups if reached == 0 => i18n.t("services.effect_staff_only").to_string(),
        Visibility::Groups => i18n.plural("services.effect_restricted", reached),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inheriting_ignores_the_other_fields() {
        let audience = Audience::from_form("", &[3], &[4], true).unwrap();
        assert!(audience.inherit);
        assert_eq!(audience.detail(), "folder");
    }

    #[test]
    fn an_unknown_or_missing_choice_is_refused_never_opened() {
        for value in ["", "anyone", "folder"] {
            let refused = Audience::from_form(value, &[], &[], false);
            assert!(matches!(refused, Err(AppError::Validation(key)) if key == VISIBILITY_CHOICE));
        }
    }

    #[test]
    fn only_a_restricted_choice_keeps_its_groups_and_members() {
        let open = Audience::from_form("everyone", &[3], &[4], false).unwrap();
        assert_eq!((open.group_ids.len(), open.member_ids.len()), (0, 0));
        let restricted = Audience::from_form("groups", &[3, 7], &[4], false).unwrap();
        assert_eq!(restricted.group_ids, vec![3, 7]);
        assert_eq!(restricted.detail(), "groups:2;members:1");
    }
}
