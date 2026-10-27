//! Outage repository: the outages the checks detected on each service.

use std::collections::HashMap;

use chrono::{DateTime, TimeDelta, Utc};
use sqlx::Sqlite;

use crate::clock;
use crate::db::DbPool;

pub struct OutageRepository;

impl OutageRepository {
    /// Outages under way or ended since `since`, by service, oldest first.
    pub async fn since(
        pool: &DbPool,
        since: DateTime<Utc>,
    ) -> Result<HashMap<i64, Vec<OutageSpan>>, sqlx::Error> {
        let rows: Vec<(i64, DateTime<Utc>, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT service_id, started_at, ended_at FROM service_outages \
             WHERE ended_at IS NULL OR ended_at >= ? ORDER BY started_at",
        )
        .bind(clock::db(since))
        .fetch_all(pool)
        .await?;
        let mut spans: HashMap<i64, Vec<OutageSpan>> = HashMap::new();
        for (service_id, start, end) in rows {
            spans
                .entry(service_id)
                .or_default()
                .push(OutageSpan { start, end });
        }
        Ok(spans)
    }

    /// The outages of one service under way or ended since `since`,
    /// newest first.
    pub async fn recent(
        pool: &DbPool,
        service_id: i64,
        since: DateTime<Utc>,
    ) -> Result<Vec<Outage>, sqlx::Error> {
        let rows: Vec<OutageRow> = sqlx::query_as(
            "SELECT id, started_at, ended_at, source, reason FROM service_outages \
             WHERE service_id = ? AND (ended_at IS NULL OR ended_at >= ?) \
             ORDER BY started_at DESC",
        )
        .bind(service_id)
        .bind(clock::db(since))
        .fetch_all(pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, start, end, source, reason)| Outage {
                id,
                start,
                end,
                cause: OutageCause::read(&source, reason),
            })
            .collect())
    }

    /// The ended outages of other services that began within `window` of an
    /// outage of `service_id` ended since `since`: what fell together, as
    /// `(its outage, the other outage, the other service)`.
    pub async fn fallen_together(
        pool: &DbPool,
        service_id: i64,
        since: DateTime<Utc>,
        window: TimeDelta,
    ) -> Result<Vec<(i64, i64, i64)>, sqlx::Error> {
        let (before, after) = (
            format!("-{} seconds", window.num_seconds()),
            format!("+{} seconds", window.num_seconds()),
        );
        // Times are stored as `%Y-%m-%d %H:%M:%S` text: compared as such,
        // the window reads the index on `started_at`.
        sqlx::query_as(
            "SELECT mine.id, other.id, other.service_id FROM service_outages mine \
             JOIN service_outages other ON other.service_id != mine.service_id \
             AND other.ended_at IS NOT NULL \
             AND other.started_at BETWEEN datetime(mine.started_at, ?) \
             AND datetime(mine.started_at, ?) \
             WHERE mine.service_id = ? AND mine.ended_at >= ?",
        )
        .bind(before)
        .bind(after)
        .bind(service_id)
        .bind(clock::db(since))
        .fetch_all(pool)
        .await
    }

    /// Erases an ended outage of `service_id`, then the ended outages
    /// `others`. Returns what was erased, the outage asked for first;
    /// nothing when it is under way or not this service's.
    pub async fn erase(
        pool: &DbPool,
        service_id: i64,
        outage_id: i64,
        others: &[i64],
    ) -> Result<Vec<ErasedOutage>, sqlx::Error> {
        let mut tx = pool.begin().await?;
        let Some(first) = erase_ended(&mut tx, outage_id, Some(service_id)).await? else {
            return Ok(Vec::new());
        };
        let mut erased = vec![first];
        for &other in others {
            erased.extend(erase_ended(&mut tx, other, None).await?);
        }
        tx.commit().await?;
        Ok(erased)
    }

    /// When each outage under way began, by service.
    pub async fn open_starts(pool: &DbPool) -> Result<HashMap<i64, DateTime<Utc>>, sqlx::Error> {
        let rows: Vec<(i64, DateTime<Utc>)> = sqlx::query_as(
            "SELECT service_id, started_at FROM service_outages WHERE ended_at IS NULL",
        )
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    /// When the latest outage of a service began, under way or ended.
    pub async fn latest_start(
        pool: &DbPool,
        service_id: i64,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT started_at FROM service_outages WHERE service_id = ? \
             ORDER BY started_at DESC LIMIT 1",
        )
        .bind(service_id)
        .fetch_optional(pool)
        .await
    }
}

/// Who found an outage, and why when Statup's own checks did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutageCause {
    /// The checks, with the words of what they found (`Finding::words`).
    /// Outages recorded before the reason was kept have none.
    Check(Option<String>),
    /// A monitoring tool, through the API.
    Report,
}

impl OutageCause {
    fn source(&self) -> &'static str {
        match self {
            Self::Check(_) => "check",
            Self::Report => "report",
        }
    }

    fn reason(&self) -> Option<&str> {
        match self {
            Self::Check(reason) => reason.as_deref(),
            Self::Report => None,
        }
    }

    fn read(source: &str, reason: Option<String>) -> Self {
        match source {
            "report" => Self::Report,
            _ => Self::Check(reason),
        }
    }
}

type OutageRow = (
    i64,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    String,
    Option<String>,
);

/// One outage of a service, as its panel lists it.
pub struct Outage {
    pub id: i64,
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>,
    pub cause: OutageCause,
}

/// One detected outage on one service, as the availability strip reads it.
pub struct OutageSpan {
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>,
}

/// An outage taken out of the history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErasedOutage {
    pub service_id: i64,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Deletes an ended outage, of `service_id` when given.
async fn erase_ended(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    id: i64,
    service_id: Option<i64>,
) -> Result<Option<ErasedOutage>, sqlx::Error> {
    let row: Option<(i64, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
        "DELETE FROM service_outages WHERE id = ? AND ended_at IS NOT NULL \
         AND (? IS NULL OR service_id = ?) RETURNING service_id, started_at, ended_at",
    )
    .bind(id)
    .bind(service_id)
    .bind(service_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|(service_id, start, end)| ErasedOutage {
        service_id,
        start,
        end,
    }))
}

pub(super) async fn open_outage(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    service_id: i64,
    started_at: DateTime<Utc>,
    cause: &OutageCause,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO service_outages (service_id, started_at, source, reason) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(service_id)
    .bind(clock::db(started_at))
    .bind(cause.source())
    .bind(cause.reason())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(super) async fn delete_open_outage(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    service_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM service_outages WHERE service_id = ? AND ended_at IS NULL")
        .bind(service_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Never ends an outage before it started, should the clock step back.
pub(super) async fn close_outage(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    service_id: i64,
    ended_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE service_outages SET ended_at = MAX(started_at, ?) \
         WHERE service_id = ? AND ended_at IS NULL",
    )
    .bind(clock::db(ended_at))
    .bind(service_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;
    use crate::repositories::ServiceRepository;
    use crate::test_helpers::test_pool;

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 4, hour, 0, 0).unwrap()
    }

    #[tokio::test]
    async fn recent_lists_the_long_outages_of_one_service_with_their_cause() {
        let pool = test_pool().await;
        let svc = ServiceRepository::create(&pool, "Mail", "mail", None, None, None)
            .await
            .unwrap();
        let other = ServiceRepository::create(&pool, "Wiki", "wiki", None, None, None)
            .await
            .unwrap();
        let checked = OutageCause::Check(Some("name not found".to_string()));
        let spans = [
            (svc.id, at(1), at(2), &checked),
            (svc.id, at(3), at(3) + Duration::minutes(5), &checked),
            (svc.id, at(4), at(6), &OutageCause::Report),
            (other.id, at(4), at(6), &checked),
        ];
        for (id, start, end, cause) in spans {
            ServiceRepository::mark_detected_down(&pool, id, start, cause)
                .await
                .unwrap();
            ServiceRepository::mark_detected_up(&pool, id, end)
                .await
                .unwrap();
        }

        let outages = OutageRepository::recent(&pool, svc.id, at(0))
            .await
            .unwrap();

        let listed: Vec<_> = outages.iter().map(|o| (o.start, o.cause.clone())).collect();
        assert_eq!(
            listed,
            [
                (at(4), OutageCause::Report),
                (at(3), checked.clone()),
                (at(1), checked)
            ]
        );
    }

    async fn service_down(
        pool: &DbPool,
        name: &str,
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
    ) -> i64 {
        let id = ServiceRepository::create(pool, name, &name.to_lowercase(), None, None, None)
            .await
            .unwrap()
            .id;
        ServiceRepository::mark_detected_down(pool, id, start, &OutageCause::Report)
            .await
            .unwrap();
        if let Some(end) = end {
            ServiceRepository::mark_detected_up(pool, id, end)
                .await
                .unwrap();
        }
        id
    }

    async fn outage_ids(pool: &DbPool) -> Vec<(i64, i64)> {
        sqlx::query_as("SELECT id, service_id FROM service_outages ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn erasing_takes_the_ended_outages_that_fell_together() {
        let pool = test_pool().await;
        let window = Duration::minutes(5);
        let mail = service_down(&pool, "Mail", at(1), Some(at(3))).await;
        let wiki = service_down(&pool, "Wiki", at(1) + Duration::minutes(2), Some(at(3))).await;
        let late = service_down(&pool, "Late", at(1) + Duration::minutes(9), Some(at(3))).await;
        let open = service_down(&pool, "Open", at(1), None).await;
        let ids = outage_ids(&pool).await;
        let mail_outage = ids[0].0;

        let together = OutageRepository::fallen_together(&pool, mail, at(0), window)
            .await
            .unwrap();
        assert_eq!(together, [(mail_outage, ids[1].0, wiki)]);

        let erased = OutageRepository::erase(&pool, mail, mail_outage, &[ids[1].0, ids[3].0])
            .await
            .unwrap();
        let services: Vec<i64> = erased.iter().map(|o| o.service_id).collect();
        assert_eq!(services, [mail, wiki]);
        let left: Vec<i64> = outage_ids(&pool).await.iter().map(|(_, s)| *s).collect();
        assert_eq!(left, [late, open]);
    }

    #[tokio::test]
    async fn an_outage_under_way_or_of_another_service_is_never_erased() {
        let pool = test_pool().await;
        let open = service_down(&pool, "Open", at(1), None).await;
        let other = service_down(&pool, "Other", at(1), Some(at(2))).await;
        let ids = outage_ids(&pool).await;

        assert_eq!(
            OutageRepository::erase(&pool, open, ids[0].0, &[])
                .await
                .unwrap(),
            []
        );
        assert_eq!(
            OutageRepository::erase(&pool, open, ids[1].0, &[])
                .await
                .unwrap(),
            []
        );
        assert_eq!(outage_ids(&pool).await.len(), 2, "{other} keeps its outage");
    }

    #[tokio::test]
    async fn since_keeps_open_and_recent_outages() {
        let pool = test_pool().await;
        let old = ServiceRepository::create(&pool, "Old", "old", None, None, None)
            .await
            .unwrap();
        let recent = ServiceRepository::create(&pool, "Recent", "recent", None, None, None)
            .await
            .unwrap();
        let open = ServiceRepository::create(&pool, "Open", "open", None, None, None)
            .await
            .unwrap();
        for (id, start, end) in [(old.id, 1, Some(2)), (recent.id, 3, Some(5))] {
            ServiceRepository::mark_detected_down(&pool, id, at(start), &OutageCause::Report)
                .await
                .unwrap();
            if let Some(end) = end {
                ServiceRepository::mark_detected_up(&pool, id, at(end))
                    .await
                    .unwrap();
            }
        }
        ServiceRepository::mark_detected_down(&pool, open.id, at(6), &OutageCause::Report)
            .await
            .unwrap();

        let spans = OutageRepository::since(&pool, at(4)).await.unwrap();
        assert!(!spans.contains_key(&old.id));
        assert_eq!(spans[&recent.id][0].start, at(3));
        assert_eq!(spans[&recent.id][0].end, Some(at(5)));
        assert_eq!(spans[&open.id][0].end, None);
    }

    #[tokio::test]
    async fn an_outage_never_ends_before_it_starts() {
        let pool = test_pool().await;
        let svc = ServiceRepository::create(&pool, "Skew", "skew", None, None, None)
            .await
            .unwrap();
        ServiceRepository::mark_detected_down(&pool, svc.id, at(10), &OutageCause::Report)
            .await
            .unwrap();
        ServiceRepository::mark_detected_up(&pool, svc.id, at(10) - Duration::minutes(5))
            .await
            .unwrap();
        let spans = OutageRepository::since(&pool, at(0)).await.unwrap();
        assert_eq!(spans[&svc.id][0].end, Some(at(10)));
    }

    #[tokio::test]
    async fn the_latest_start_is_the_newest_outage() {
        let pool = test_pool().await;
        let svc = ServiceRepository::create(&pool, "Mail", "mail", None, None, None)
            .await
            .unwrap();
        assert_eq!(
            OutageRepository::latest_start(&pool, svc.id).await.unwrap(),
            None
        );
        for (start, end) in [(1, 2), (5, 6)] {
            ServiceRepository::mark_detected_down(&pool, svc.id, at(start), &OutageCause::Report)
                .await
                .unwrap();
            ServiceRepository::mark_detected_up(&pool, svc.id, at(end))
                .await
                .unwrap();
        }

        assert_eq!(
            OutageRepository::latest_start(&pool, svc.id).await.unwrap(),
            Some(at(5))
        );
    }
}
