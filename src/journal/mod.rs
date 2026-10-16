//! The journal: each line is written once and goes to two places, the
//! server output and the Journal page of the administrators.
//!
//! What a member does or what the app finds is recorded on purpose by
//! [`record`]; the errors and warnings the app logs anywhere else are
//! caught by [`JournalLayer`].

mod layer;
mod writer;

use crate::db::DbPool;
use crate::models::User;
use crate::repositories::{JournalRepository, NewJournalEntry};

pub use layer::{JournalInbox, JournalLayer, journal_layer};
pub use writer::{DEFAULT_RETENTION_DAYS, RETENTION_SETTING, spawn_journal_writer};

/// The target of the lines [`record`] writes to the server output, which
/// [`JournalLayer`] leaves alone so that nothing is kept twice.
pub const TARGET: &str = "statup::journal";

/// A detail longer than this is cut.
const DETAIL_LIMIT: usize = 1000;

impl NewJournalEntry {
    /// The member who did it, named as they were at the time.
    #[must_use]
    pub fn by(mut self, member: &User) -> Self {
        self.actor_id = Some(member.id);
        self.actor_name = Some(member.display_name.clone());
        self
    }

    /// What it was done to: a service, an event, a member, a channel.
    #[must_use]
    pub fn about(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// What a reader needs to understand it, in English.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// The address the request came from, kept for sign-ins only.
    #[must_use]
    pub fn from_ip(mut self, ip: impl Into<String>) -> Self {
        self.ip = Some(ip.into());
        self
    }
}

/// Writes a line to the server output and keeps it for the Journal page.
/// A line that cannot be kept never fails the action it records.
pub async fn record(pool: &DbPool, mut entry: NewJournalEntry) {
    entry.detail = entry.detail.as_deref().map(scrub);
    write_line(&entry);
    if let Err(error) = JournalRepository::add(pool, &entry).await {
        tracing::warn!(target: TARGET, %error, "Journal line not saved");
    }
}

fn write_line(entry: &NewJournalEntry) {
    macro_rules! line {
        ($macro:ident) => {
            tracing::$macro!(
                target: TARGET,
                kind = ?entry.kind,
                actor = entry.actor_name.as_deref(),
                subject = entry.subject.as_deref(),
                detail = entry.detail.as_deref(),
                ip = entry.ip.as_deref(),
                "{}",
                entry.kind.summary(),
            )
        };
    }
    match entry.kind.level() {
        tracing::Level::ERROR => line!(error),
        tracing::Level::WARN => line!(warn),
        tracing::Level::INFO => line!(info),
        _ => line!(debug),
    }
}

/// Keeps only the scheme and host of every web address, so that a webhook
/// token, a password in an address or a key in a checked address never
/// reaches the journal, and cuts the text to [`DETAIL_LIMIT`].
pub fn scrub(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = address_start(rest) {
        clean.push_str(&rest[..start]);
        let address = &rest[start..];
        let end = address
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')' | '`'))
            .unwrap_or(address.len());
        clean.push_str(&origin_of(&address[..end]));
        rest = &address[end..];
    }
    clean.push_str(rest);
    cut(clean)
}

fn address_start(text: &str) -> Option<usize> {
    [text.find("http://"), text.find("https://")]
        .into_iter()
        .flatten()
        .min()
}

/// `https://user:secret@host:8080/path?key=1` becomes `https://host:8080/…`.
fn origin_of(address: &str) -> String {
    let scheme_end = address.find("://").map_or(0, |at| at + 3);
    let authority_end = address[scheme_end..]
        .find(['/', '?', '#'])
        .map_or(address.len(), |at| scheme_end + at);
    let authority = &address[scheme_end..authority_end];
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let mut origin = format!("{}{host}", &address[..scheme_end]);
    if authority_end < address.len() {
        origin.push_str("/…");
    }
    origin
}

fn cut(text: String) -> String {
    match text.char_indices().nth(DETAIL_LIMIT) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::JournalKind;
    use crate::test_helpers::test_pool;

    #[test]
    fn an_address_keeps_only_its_scheme_and_host() {
        assert_eq!(
            scrub(
                "error sending request for url \
                 (https://hooks.slack.com/services/T000/B000/XXXXSECRET)"
            ),
            "error sending request for url (https://hooks.slack.com/…)"
        );
        assert_eq!(
            scrub("got http://admin:hunter2@10.0.0.5:8080?token=abc and https://intranet"),
            "got http://10.0.0.5:8080/… and https://intranet"
        );
    }

    #[test]
    fn text_without_address_is_kept_and_a_long_one_is_cut() {
        assert_eq!(scrub("Database is locked"), "Database is locked");
        let long = "é".repeat(DETAIL_LIMIT + 10);
        let cut = scrub(&long);
        assert_eq!(cut.chars().count(), DETAIL_LIMIT + 1);
        assert!(cut.ends_with('…'));
    }

    #[tokio::test]
    async fn a_recorded_line_is_kept_scrubbed() {
        let pool = test_pool().await;
        let entry = NewJournalEntry::new(JournalKind::AppError)
            .about("Delivery")
            .with_detail("refused by https://example.com/hook/SECRET");

        record(&pool, entry).await;

        let detail: Option<String> = sqlx::query_scalar("SELECT detail FROM journal_entries")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(detail.as_deref(), Some("refused by https://example.com/…"));
    }
}
