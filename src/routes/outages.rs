//! The outages the checks or a monitoring tool found on a service, as its
//! side panel lists them.

use chrono::{DateTime, Utc};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

use super::journal::finding_words;
use crate::clock;
use crate::db::DbPool;
use crate::error::AppError;
use crate::i18n::I18n;
use crate::models::{User, split_duration};
use crate::modules::services::{MIN_DETECTED_OUTAGE, SHORT_DAYS};
use crate::repositories::{Outage, OutageCause, OutageRepository};

/// Most outages a service's panel lists.
const PANEL_OUTAGES: usize = 5;

/// One outage as the panel shows it, cut to what the viewer may read.
pub struct OutageLine {
    /// "Du 26 oct. à 23:56 au 27 oct. à 08:42".
    pub when: String,
    /// "8 h 46 min".
    pub length: String,
    /// What the checks found, for members.
    pub reason: Option<String>,
    /// The Journal around the outage, for administrators.
    pub journal: Option<String>,
}

/// The outages that coloured the days of a service's strip, newest first.
pub async fn panel_outages(
    pool: &DbPool,
    service_id: i64,
    service_name: &str,
    user: Option<&User>,
    i18n: &I18n,
) -> Result<Vec<OutageLine>, AppError> {
    let now = Utc::now();
    let since = now - chrono::Duration::days(SHORT_DAYS);
    let outages =
        OutageRepository::recent(pool, service_id, since, MIN_DETECTED_OUTAGE, now).await?;
    Ok(outages
        .into_iter()
        .take(PANEL_OUTAGES)
        .map(|outage| line(&outage, service_name, user, now, i18n))
        .collect())
}

fn line(
    outage: &Outage,
    service_name: &str,
    user: Option<&User>,
    now: DateTime<Utc>,
    i18n: &I18n,
) -> OutageLine {
    let end = outage.end.unwrap_or(now);
    OutageLine {
        when: when(outage, i18n),
        length: i18n.format_duration(&split_duration(end - outage.start)),
        reason: user.and_then(|_| reason(&outage.cause, i18n)),
        journal: user
            .filter(|u| u.role.can_admin())
            .map(|_| journal_link(outage.start, end, service_name)),
    }
}

/// "Depuis le 27 oct. à 08:12", "27 oct., de 03:00 à 03:40" or "Du 26
/// oct. à 23:56 au 27 oct. à 08:42".
fn when(outage: &Outage, i18n: &I18n) -> String {
    let Some(end) = outage.end else {
        let start = i18n.format_datetime(&outage.start);
        return i18n.tf("services.outage_since", &[("start", &start)]);
    };
    let day = clock::local_date(&outage.start);
    if day == clock::local_date(&end) {
        return i18n.tf(
            "services.outage_within_day",
            &[
                ("date", &i18n.format_date_short(&day)),
                ("start", &i18n.format_time(&outage.start)),
                ("end", &i18n.format_time(&end)),
            ],
        );
    }
    i18n.tf(
        "services.outage_across_days",
        &[
            ("start", &i18n.format_datetime(&outage.start)),
            ("end", &i18n.format_datetime(&end)),
        ],
    )
}

fn reason(cause: &OutageCause, i18n: &I18n) -> Option<String> {
    match cause {
        OutageCause::Check(words) => words.as_deref().and_then(|w| finding_words(w, i18n)),
        OutageCause::Report => Some(i18n.t("services.outage_reported").to_string()),
    }
}

/// The monitoring lines of the days the outage covered, about this service.
fn journal_link(start: DateTime<Utc>, end: DateTime<Utc>, service_name: &str) -> String {
    let day = |at: &DateTime<Utc>| clock::local_date(at).format("%Y-%m-%d").to_string();
    format!(
        "/admin/journal?category=monitoring&from={}&to={}&q={}",
        day(&start),
        day(&end),
        utf8_percent_encode(service_name, NON_ALPHANUMERIC)
    )
}
