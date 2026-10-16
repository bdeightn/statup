//! The journal: what members did and what the app met, kept for the
//! administrators.

use tracing::Level;

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

    /// The line in the server output, in English.
    pub fn summary(self) -> &'static str {
        match self {
            Self::AppError => "Application error",
            Self::AppWarning => "Application warning",
        }
    }
}
