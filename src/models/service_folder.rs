//! A folder of services, with its own choice of who sees them.

use super::Visibility;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ServiceFolder {
    pub id: i64,
    pub name: String,
    pub position: i64,
    pub visibility: Visibility,
}

/// Longest folder name, in characters.
pub const FOLDER_NAME_MAX_CHARS: usize = 100;

/// The name as kept, trimmed, or the key of what is wrong with it.
pub fn check_folder_name(raw: &str) -> Result<String, &'static str> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("validation.folder_name_required");
    }
    if name.chars().count() > FOLDER_NAME_MAX_CHARS {
        return Err("validation.folder_name_too_long");
    }
    if name.chars().any(char::is_control) {
        return Err("validation.folder_name_invalid");
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_name_is_trimmed_and_checked() {
        assert_eq!(check_folder_name("  Paie "), Ok("Paie".to_string()));
        assert_eq!(
            check_folder_name("   "),
            Err("validation.folder_name_required")
        );
        assert_eq!(
            check_folder_name(&"a".repeat(FOLDER_NAME_MAX_CHARS + 1)),
            Err("validation.folder_name_too_long")
        );
        assert_eq!(
            check_folder_name("Pa\u{7}ie"),
            Err("validation.folder_name_invalid")
        );
    }
}
