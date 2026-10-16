//! The journal: what members did and what the app met, kept for the
//! administrators.

use chrono::{DateTime, Utc};
use tracing::Level;

use super::Tone;
use crate::clock;
use crate::i18n::I18n;

/// The family of a line, offered as a filter on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum JournalCategory {
    /// Changes made by a member.
    Members,
    /// Sign-ins and password resets.
    Access,
    /// What the checks of the services found.
    Monitoring,
    /// Messages to the alert channels.
    Alerts,
    /// Errors and warnings of the app itself.
    System,
}

/// What a line records, which decides its family, whether the default view
/// shows it, and its words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum JournalKind {
    /// An error the app logged.
    AppError,
    /// A warning the app logged.
    AppWarning,
}

impl JournalKind {
    pub fn category(self) -> JournalCategory {
        match self {
            Self::AppError | Self::AppWarning => JournalCategory::System,
        }
    }

    /// The default view keeps the large changes and the real problems.
    pub fn is_essential(self) -> bool {
        match self {
            Self::AppError => true,
            Self::AppWarning => false,
        }
    }

    /// The level of its line in the server output.
    pub fn level(self) -> Level {
        match self {
            Self::AppError => Level::ERROR,
            Self::AppWarning => Level::WARN,
        }
    }

    /// The hue of its chip: color only for problems.
    pub fn tone(self) -> Tone {
        match self {
            Self::AppError => Tone::Crit,
            Self::AppWarning => Tone::Minor,
        }
    }

    /// The line in the server output, in English.
    pub fn summary(self) -> &'static str {
        match self {
            Self::AppError => "Application error",
            Self::AppWarning => "Application warning",
        }
    }
}

impl JournalCategory {
    pub const ALL: [Self; 5] = [
        Self::Members,
        Self::Access,
        Self::Monitoring,
        Self::Alerts,
        Self::System,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Members => "members",
            Self::Access => "access",
            Self::Monitoring => "monitoring",
            Self::Alerts => "alerts",
            Self::System => "system",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.as_str() == value)
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Members => "journal.category_members",
            Self::Access => "journal.category_access",
            Self::Monitoring => "journal.category_monitoring",
            Self::Alerts => "journal.category_alerts",
            Self::System => "journal.category_system",
        }
    }
}

/// A line as the Journal page lists it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct JournalEntry {
    pub id: i64,
    pub kind: JournalKind,
    pub category: JournalCategory,
    pub actor_name: Option<String>,
    pub subject: Option<String>,
    pub detail: Option<String>,
    pub ip: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl JournalEntry {
    /// What happened, in the reader's language.
    pub fn sentence(&self, i18n: &I18n) -> String {
        let key = match self.kind {
            JournalKind::AppError => "journal.app_error",
            JournalKind::AppWarning => "journal.app_warning",
        };
        i18n.tf(
            key,
            &[
                ("actor", self.actor_name.as_deref().unwrap_or_default()),
                ("subject", self.subject.as_deref().unwrap_or_default()),
            ],
        )
    }

    /// The facts under the sentence: the detail, then the address.
    pub fn facts(&self) -> Vec<&str> {
        [self.detail.as_deref(), self.ip.as_deref()]
            .into_iter()
            .flatten()
            .filter(|fact| !fact.is_empty())
            .collect()
    }
}

/// What the Journal page is narrowed to.
#[derive(Debug, Clone, Default)]
pub struct JournalFilters {
    pub category: Option<JournalCategory>,
    pub actor_id: Option<i64>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub q: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

/// The lines of one day, newest first.
pub struct JournalDay {
    pub label: String,
    pub entries: Vec<JournalEntry>,
}

/// Groups lines by local day, keeping their order. Expects newest first.
pub fn group_journal_by_day(entries: Vec<JournalEntry>, i18n: &I18n) -> Vec<JournalDay> {
    let mut days: Vec<JournalDay> = Vec::new();
    let mut current = None;
    for entry in entries {
        let date = clock::local_date(&entry.created_at);
        if current != Some(date) {
            days.push(JournalDay {
                label: i18n.date_label(&date),
                entries: Vec::new(),
            });
            current = Some(date);
        }
        if let Some(day) = days.last_mut() {
            day.entries.push(entry);
        }
    }
    days
}
