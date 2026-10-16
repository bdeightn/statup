//! Event pages: the list and its search, the event page and its side panel,
//! the forms, updates, state changes, reviews and templates.

use serde::de::value::{I64Deserializer, StrDeserializer};
use serde::{Deserialize, Deserializer};

mod detail;
mod form;
mod list;
mod review;
mod templates;
mod updates;

pub use detail::{detail, drawer_content};
pub use form::{create, edit_form, new_form, update};
pub use list::list;
pub use review::{publish_review, remove_review};
pub use templates::{template_delete, template_detail, template_search};
pub use updates::{add_update, add_update_in_panel, delete, delete_update, revert_lifecycle};

/// A field left blank means none. A filled one is read as text, or as a
/// number when the field holds one, since a form sends everything as text.
pub(crate) fn deserialize_blank_as_none<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let raw = String::deserialize(deserializer)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match T::deserialize(StrDeserializer::<D::Error>::new(&raw)) {
        Ok(value) => Ok(Some(value)),
        Err(error) => match trimmed.parse::<i64>() {
            Ok(number) => T::deserialize(I64Deserializer::<D::Error>::new(number)).map(Some),
            Err(_) => Err(error),
        },
    }
}
