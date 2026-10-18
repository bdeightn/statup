//! A named set of members, used to choose who sees a service.

/// Longest group name, in characters.
pub const GROUP_NAME_MAX_CHARS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct MemberGroup {
    pub id: i64,
    pub name: String,
    /// Active members only; filled by the listing query.
    pub member_count: i64,
}

/// A member as a group's page lists them.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct GroupMember {
    pub id: i64,
    pub display_name: String,
    pub email: String,
    pub is_active: bool,
}

/// The name as kept, trimmed, or the key of what is wrong with it.
pub fn check_group_name(raw: &str) -> Result<String, &'static str> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("validation.group_name_required");
    }
    if name.chars().count() > GROUP_NAME_MAX_CHARS {
        return Err("validation.group_name_too_long");
    }
    if name.chars().any(char::is_control) {
        return Err("validation.group_name_invalid");
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_name_is_trimmed_and_bounded() {
        assert_eq!(check_group_name("  Compta  "), Ok("Compta".to_string()));
        assert_eq!(
            check_group_name("   "),
            Err("validation.group_name_required")
        );
        assert_eq!(check_group_name(&"é".repeat(100)), Ok("é".repeat(100)));
        assert_eq!(
            check_group_name(&"a".repeat(101)),
            Err("validation.group_name_too_long")
        );
        assert_eq!(
            check_group_name("Paie\u{7}"),
            Err("validation.group_name_invalid")
        );
    }
}
