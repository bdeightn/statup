//! A key that lets a script or a monitoring tool act through the API.

use chrono::{DateTime, Utc};

/// Longest key name, in characters.
pub const API_KEY_NAME_MAX_CHARS: usize = 60;

/// What every key starts with, so a leaked key is easy to recognise.
pub const API_KEY_PREFIX: &str = "stp_";

/// A key in use, as the keys page lists it. Holds no secret.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ApiKey {
    pub id: i64,
    pub name: String,
    /// The first characters of the key.
    pub prefix: String,
    pub creator_name: String,
    /// A key keeps working when its creator's account is disabled; the page
    /// says so, for an administrator to decide.
    pub creator_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// The key a request presented, and who answers for what it writes.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ApiKeyHolder {
    pub id: i64,
    pub name: String,
    /// The administrator who made the key: the author of what it publishes.
    pub created_by: i64,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// The name as kept, trimmed, or the key of what is wrong with it.
pub fn check_api_key_name(raw: &str) -> Result<String, &'static str> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("validation.api_key_name_required");
    }
    if name.chars().count() > API_KEY_NAME_MAX_CHARS {
        return Err("validation.api_key_name_too_long");
    }
    if name.chars().any(char::is_control) {
        return Err("validation.api_key_name_invalid");
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_name_is_trimmed_and_bounded() {
        assert_eq!(check_api_key_name("  Zabbix  "), Ok("Zabbix".to_string()));
        assert_eq!(
            check_api_key_name("  "),
            Err("validation.api_key_name_required")
        );
        assert_eq!(check_api_key_name(&"é".repeat(60)), Ok("é".repeat(60)));
        assert_eq!(
            check_api_key_name(&"a".repeat(61)),
            Err("validation.api_key_name_too_long")
        );
        assert_eq!(
            check_api_key_name("Zab\u{7}bix"),
            Err("validation.api_key_name_invalid")
        );
    }
}
