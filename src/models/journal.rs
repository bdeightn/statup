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
    /// A member added a service.
    ServiceCreated,
    /// A member changed a service.
    ServiceUpdated,
    /// A member deleted a service.
    ServiceDeleted,
    /// A member set the status of a service.
    ServiceStatusSet,
    /// A member changed the order of the services.
    ServicesReordered,
    /// A member turned the problems-first order on or off.
    ProblemsFirstChanged,
    /// A member saved the general settings.
    SettingsSaved,
    /// A member removed the logo.
    LogoRemoved,
    /// A member changed what the journal keeps.
    JournalSettingsSaved,
    /// A member changed the address of the page used in alerts.
    PageAddressSaved,
    /// A member added an alert channel.
    ChannelCreated,
    /// A member changed an alert channel.
    ChannelUpdated,
    /// A member deleted an alert channel.
    ChannelDeleted,
    /// A member tried an alert channel.
    ChannelTested,
    /// A member added someone to the team.
    MemberAdded,
    /// A member changed the role of someone.
    MemberRoleChanged,
    /// A member disabled an account.
    MemberDisabled,
    /// A member enabled an account again.
    MemberEnabled,
    /// A member gave someone a temporary password.
    MemberPasswordReset,
    /// A member published an event.
    EventCreated,
    /// A member edited an event.
    EventEdited,
    /// A member posted news on an event.
    EventUpdatePosted,
    /// A member removed an update of an event.
    EventUpdateDeleted,
    /// A member reopened an event.
    EventReverted,
    /// A member deleted an event.
    EventDeleted,
    /// A member published the review of an incident.
    ReviewPublished,
    /// A member removed the review of an incident.
    ReviewRemoved,
    /// A member saved an event template.
    TemplateSaved,
    /// A member deleted an event template.
    TemplateDeleted,
    /// A member added an icon.
    IconUploaded,
    /// A member deleted an icon.
    IconDeleted,
    /// A member arranged the dashboard.
    DashboardArranged,
    /// A member changed their profile.
    ProfileUpdated,
    /// A member changed their own password.
    OwnPasswordChanged,
    /// Someone signed in.
    SignedIn,
    /// A sign-in was refused.
    SignInFailed,
    /// Too many failed sign-ins from one address.
    SignInBlocked,
    /// Too many wrong current passwords for one account.
    PasswordCheckBlocked,
    /// Someone asked for a link to choose a new password.
    PasswordResetRequested,
    /// Too many requests for a link from one address, or for one account.
    PasswordResetBlocked,
    /// The email with the link to a new password could not be sent.
    PasswordResetNotSent,
    /// Someone chose a new password through the emailed link.
    PasswordResetCompleted,
    /// The checks declared a service down.
    OutageDetected,
    /// A service declared down answers again.
    OutageEnded,
    /// One check of a service failed.
    CheckFailed,
    /// Every service failed at once: the round was ignored.
    MonitoringRoundSkipped,
    /// A message left for an alert channel.
    AlertSent,
    /// A message did not leave and will be tried again.
    AlertRetrying,
    /// A message was given up.
    AlertAbandoned,
}

impl JournalKind {
    pub fn category(self) -> JournalCategory {
        match self {
            Self::AppError | Self::AppWarning => JournalCategory::System,
            Self::ServiceCreated
            | Self::ServiceUpdated
            | Self::ServiceDeleted
            | Self::ServiceStatusSet
            | Self::ServicesReordered
            | Self::ProblemsFirstChanged
            | Self::SettingsSaved
            | Self::LogoRemoved
            | Self::JournalSettingsSaved
            | Self::PageAddressSaved
            | Self::ChannelCreated
            | Self::ChannelUpdated
            | Self::ChannelDeleted
            | Self::ChannelTested
            | Self::MemberAdded
            | Self::MemberRoleChanged
            | Self::MemberDisabled
            | Self::MemberEnabled
            | Self::MemberPasswordReset
            | Self::EventCreated
            | Self::EventEdited
            | Self::EventUpdatePosted
            | Self::EventUpdateDeleted
            | Self::EventReverted
            | Self::EventDeleted
            | Self::ReviewPublished
            | Self::ReviewRemoved
            | Self::TemplateSaved
            | Self::TemplateDeleted
            | Self::IconUploaded
            | Self::IconDeleted
            | Self::DashboardArranged
            | Self::ProfileUpdated
            | Self::OwnPasswordChanged => JournalCategory::Members,
            Self::SignedIn
            | Self::SignInFailed
            | Self::SignInBlocked
            | Self::PasswordCheckBlocked
            | Self::PasswordResetRequested
            | Self::PasswordResetBlocked
            | Self::PasswordResetNotSent
            | Self::PasswordResetCompleted => JournalCategory::Access,
            Self::OutageDetected
            | Self::OutageEnded
            | Self::CheckFailed
            | Self::MonitoringRoundSkipped => JournalCategory::Monitoring,
            Self::AlertSent | Self::AlertRetrying | Self::AlertAbandoned => JournalCategory::Alerts,
        }
    }

    /// The default view keeps the large changes and the real problems.
    pub fn is_essential(self) -> bool {
        match self {
            Self::AppError
            | Self::ServiceCreated
            | Self::ServiceUpdated
            | Self::ServiceDeleted
            | Self::SettingsSaved
            | Self::LogoRemoved
            | Self::JournalSettingsSaved
            | Self::PageAddressSaved
            | Self::ChannelCreated
            | Self::ChannelUpdated
            | Self::ChannelDeleted
            | Self::MemberAdded
            | Self::MemberRoleChanged
            | Self::MemberDisabled
            | Self::MemberEnabled
            | Self::MemberPasswordReset
            | Self::EventDeleted
            | Self::SignInBlocked
            | Self::PasswordCheckBlocked
            | Self::PasswordResetBlocked
            | Self::PasswordResetNotSent
            | Self::OutageDetected
            | Self::OutageEnded
            | Self::MonitoringRoundSkipped
            | Self::AlertAbandoned => true,
            Self::AppWarning
            | Self::ServiceStatusSet
            | Self::ServicesReordered
            | Self::ProblemsFirstChanged
            | Self::ChannelTested
            | Self::EventCreated
            | Self::EventEdited
            | Self::EventUpdatePosted
            | Self::EventUpdateDeleted
            | Self::EventReverted
            | Self::ReviewPublished
            | Self::ReviewRemoved
            | Self::TemplateSaved
            | Self::TemplateDeleted
            | Self::IconUploaded
            | Self::IconDeleted
            | Self::DashboardArranged
            | Self::ProfileUpdated
            | Self::OwnPasswordChanged
            | Self::SignedIn
            | Self::SignInFailed
            | Self::PasswordResetRequested
            | Self::PasswordResetCompleted
            | Self::CheckFailed
            | Self::AlertSent
            | Self::AlertRetrying => false,
        }
    }

    /// The level of its line in the server output.
    pub fn level(self) -> Level {
        match self {
            Self::AppError => Level::ERROR,
            Self::ServiceCreated
            | Self::ServiceUpdated
            | Self::ServiceDeleted
            | Self::ServiceStatusSet
            | Self::ServicesReordered
            | Self::ProblemsFirstChanged
            | Self::SettingsSaved
            | Self::LogoRemoved
            | Self::JournalSettingsSaved
            | Self::PageAddressSaved
            | Self::ChannelCreated
            | Self::ChannelUpdated
            | Self::ChannelDeleted
            | Self::ChannelTested
            | Self::MemberAdded
            | Self::MemberRoleChanged
            | Self::MemberDisabled
            | Self::MemberEnabled
            | Self::MemberPasswordReset
            | Self::EventCreated
            | Self::EventEdited
            | Self::EventUpdatePosted
            | Self::EventUpdateDeleted
            | Self::EventReverted
            | Self::EventDeleted
            | Self::ReviewPublished
            | Self::ReviewRemoved
            | Self::TemplateSaved
            | Self::TemplateDeleted
            | Self::IconUploaded
            | Self::IconDeleted
            | Self::DashboardArranged
            | Self::ProfileUpdated
            | Self::OwnPasswordChanged
            | Self::SignedIn
            | Self::PasswordResetRequested
            | Self::PasswordResetCompleted
            | Self::OutageEnded => Level::INFO,
            Self::CheckFailed | Self::AlertSent => Level::DEBUG,
            Self::AppWarning
            | Self::SignInFailed
            | Self::SignInBlocked
            | Self::PasswordCheckBlocked
            | Self::PasswordResetBlocked
            | Self::PasswordResetNotSent
            | Self::OutageDetected
            | Self::MonitoringRoundSkipped
            | Self::AlertRetrying
            | Self::AlertAbandoned => Level::WARN,
        }
    }

    /// The hue of its chip: color only for problems.
    pub fn tone(self) -> Tone {
        match self {
            Self::AppError | Self::OutageDetected | Self::AlertAbandoned => Tone::Crit,
            Self::OutageEnded => Tone::Ok,
            Self::ServiceCreated
            | Self::ServiceUpdated
            | Self::ServiceDeleted
            | Self::ServiceStatusSet
            | Self::ServicesReordered
            | Self::ProblemsFirstChanged
            | Self::SettingsSaved
            | Self::LogoRemoved
            | Self::JournalSettingsSaved
            | Self::PageAddressSaved
            | Self::ChannelCreated
            | Self::ChannelUpdated
            | Self::ChannelDeleted
            | Self::ChannelTested
            | Self::MemberAdded
            | Self::MemberRoleChanged
            | Self::MemberDisabled
            | Self::MemberEnabled
            | Self::MemberPasswordReset
            | Self::EventCreated
            | Self::EventEdited
            | Self::EventUpdatePosted
            | Self::EventUpdateDeleted
            | Self::EventReverted
            | Self::EventDeleted
            | Self::ReviewPublished
            | Self::ReviewRemoved
            | Self::TemplateSaved
            | Self::TemplateDeleted
            | Self::IconUploaded
            | Self::IconDeleted
            | Self::DashboardArranged
            | Self::ProfileUpdated
            | Self::OwnPasswordChanged
            | Self::SignedIn
            | Self::SignInFailed
            | Self::PasswordResetRequested
            | Self::PasswordResetCompleted
            | Self::AlertSent => Tone::Neutral,
            Self::AppWarning
            | Self::SignInBlocked
            | Self::PasswordCheckBlocked
            | Self::PasswordResetBlocked
            | Self::PasswordResetNotSent
            | Self::CheckFailed
            | Self::MonitoringRoundSkipped
            | Self::AlertRetrying => Tone::Minor,
        }
    }

    /// The line in the server output, in English.
    pub fn summary(self) -> &'static str {
        match self {
            Self::AppError => "Application error",
            Self::AppWarning => "Application warning",
            Self::ServiceCreated => "Service created",
            Self::ServiceUpdated => "Service updated",
            Self::ServiceDeleted => "Service deleted",
            Self::ServiceStatusSet => "Service status set",
            Self::ServicesReordered => "Services reordered",
            Self::ProblemsFirstChanged => "Problems first changed",
            Self::SettingsSaved => "Settings saved",
            Self::LogoRemoved => "Logo removed",
            Self::JournalSettingsSaved => "Journal settings saved",
            Self::PageAddressSaved => "Page address saved",
            Self::ChannelCreated => "Alert channel created",
            Self::ChannelUpdated => "Alert channel updated",
            Self::ChannelDeleted => "Alert channel deleted",
            Self::ChannelTested => "Alert channel tested",
            Self::MemberAdded => "Member added",
            Self::MemberRoleChanged => "Member role changed",
            Self::MemberDisabled => "Member disabled",
            Self::MemberEnabled => "Member enabled",
            Self::MemberPasswordReset => "Member password reset",
            Self::EventCreated => "Event created",
            Self::EventEdited => "Event edited",
            Self::EventUpdatePosted => "Event update posted",
            Self::EventUpdateDeleted => "Event update deleted",
            Self::EventReverted => "Event reverted",
            Self::EventDeleted => "Event deleted",
            Self::ReviewPublished => "Review published",
            Self::ReviewRemoved => "Review removed",
            Self::TemplateSaved => "Template saved",
            Self::TemplateDeleted => "Template deleted",
            Self::IconUploaded => "Icon uploaded",
            Self::IconDeleted => "Icon deleted",
            Self::DashboardArranged => "Dashboard arranged",
            Self::ProfileUpdated => "Profile updated",
            Self::OwnPasswordChanged => "Own password changed",
            Self::SignedIn => "Signed in",
            Self::SignInFailed => "Sign-in refused",
            Self::SignInBlocked => "Sign-ins blocked",
            Self::PasswordCheckBlocked => "Password checks blocked",
            Self::PasswordResetRequested => "Password reset requested",
            Self::PasswordResetBlocked => "Password reset requests blocked",
            Self::PasswordResetNotSent => "Password reset email not sent",
            Self::PasswordResetCompleted => "Password reset completed",
            Self::OutageDetected => "Outage detected",
            Self::OutageEnded => "Service back up",
            Self::CheckFailed => "Check failed",
            Self::MonitoringRoundSkipped => {
                "Monitoring round ignored: every service failed at once"
            }
            Self::AlertSent => "Alert sent",
            Self::AlertRetrying => "Alert not sent, will try again",
            Self::AlertAbandoned => "Alert given up",
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
            JournalKind::ServiceCreated => "journal.service_created",
            JournalKind::ServiceUpdated => "journal.service_updated",
            JournalKind::ServiceDeleted => "journal.service_deleted",
            JournalKind::ServiceStatusSet => "journal.service_status_set",
            JournalKind::ServicesReordered => "journal.services_reordered",
            JournalKind::ProblemsFirstChanged => "journal.problems_first_changed",
            JournalKind::SettingsSaved => "journal.settings_saved",
            JournalKind::LogoRemoved => "journal.logo_removed",
            JournalKind::JournalSettingsSaved => "journal.journal_settings_saved",
            JournalKind::PageAddressSaved => "journal.page_address_saved",
            JournalKind::ChannelCreated => "journal.channel_created",
            JournalKind::ChannelUpdated => "journal.channel_updated",
            JournalKind::ChannelDeleted => "journal.channel_deleted",
            JournalKind::ChannelTested => "journal.channel_tested",
            JournalKind::MemberAdded => "journal.member_added",
            JournalKind::MemberRoleChanged => "journal.member_role_changed",
            JournalKind::MemberDisabled => "journal.member_disabled",
            JournalKind::MemberEnabled => "journal.member_enabled",
            JournalKind::MemberPasswordReset => "journal.member_password_reset",
            JournalKind::EventCreated => "journal.event_created",
            JournalKind::EventEdited => "journal.event_edited",
            JournalKind::EventUpdatePosted => "journal.event_update_posted",
            JournalKind::EventUpdateDeleted => "journal.event_update_deleted",
            JournalKind::EventReverted => "journal.event_reverted",
            JournalKind::EventDeleted => "journal.event_deleted",
            JournalKind::ReviewPublished => "journal.review_published",
            JournalKind::ReviewRemoved => "journal.review_removed",
            JournalKind::TemplateSaved => "journal.template_saved",
            JournalKind::TemplateDeleted => "journal.template_deleted",
            JournalKind::IconUploaded => "journal.icon_uploaded",
            JournalKind::IconDeleted => "journal.icon_deleted",
            JournalKind::DashboardArranged => "journal.dashboard_arranged",
            JournalKind::ProfileUpdated => "journal.profile_updated",
            JournalKind::OwnPasswordChanged => "journal.own_password_changed",
            JournalKind::SignedIn => "journal.signed_in",
            JournalKind::SignInFailed if self.subject.is_none() => "journal.sign_in_failed_unknown",
            JournalKind::SignInFailed => "journal.sign_in_failed",
            JournalKind::SignInBlocked => "journal.sign_in_blocked",
            JournalKind::PasswordCheckBlocked => "journal.password_check_blocked",
            JournalKind::PasswordResetRequested => "journal.password_reset_requested",
            JournalKind::PasswordResetBlocked => "journal.password_reset_blocked",
            JournalKind::PasswordResetNotSent => "journal.password_reset_not_sent",
            JournalKind::PasswordResetCompleted => "journal.password_reset_completed",
            JournalKind::OutageDetected => "journal.outage_detected",
            JournalKind::OutageEnded => "journal.outage_ended",
            JournalKind::CheckFailed => "journal.check_failed",
            JournalKind::MonitoringRoundSkipped => "journal.monitoring_round_skipped",
            JournalKind::AlertSent => "journal.alert_sent",
            JournalKind::AlertRetrying => "journal.alert_retrying",
            JournalKind::AlertAbandoned => "journal.alert_abandoned",
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
