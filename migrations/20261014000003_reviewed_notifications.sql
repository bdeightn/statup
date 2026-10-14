-- A message can tell that the review of a resolved incident was published.
-- SQLite cannot change a CHECK in place, so the queue is rebuilt with the
-- new value, every message kept with its number. SQLite's twelve-step recipe
-- is shortened on purpose: sqlx runs this file in a transaction, where
-- `PRAGMA foreign_keys` cannot be switched off, and no table, trigger or
-- view refers to `notification_deliveries`, so dropping it breaks nothing.
CREATE TABLE notification_deliveries_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL REFERENCES notification_channels(id) ON DELETE CASCADE,
    happening TEXT NOT NULL
        CHECK (happening IN ('opened', 'updated', 'rescheduled', 'started', 'closed',
                             'published', 'reviewed', 'service_down', 'service_up', 'test')),
    event_id INTEGER REFERENCES events(id) ON DELETE CASCADE,
    update_id INTEGER REFERENCES event_updates(id) ON DELETE CASCADE,
    lifecycle TEXT,
    service_id INTEGER REFERENCES services(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'sent', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT NOT NULL DEFAULT (datetime('now')),
    failure TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    sent_at TEXT,
    CHECK ((happening IN ('service_down', 'service_up')) = (service_id IS NOT NULL)),
    CHECK ((happening IN ('service_down', 'service_up', 'test')) = (event_id IS NULL))
);

INSERT INTO notification_deliveries_new (id, channel_id, happening, event_id, update_id,
    lifecycle, service_id, status, attempts, next_attempt_at, failure, created_at, sent_at)
SELECT id, channel_id, happening, event_id, update_id, lifecycle, service_id, status,
    attempts, next_attempt_at, failure, created_at, sent_at
FROM notification_deliveries;

DROP TABLE notification_deliveries;

ALTER TABLE notification_deliveries_new RENAME TO notification_deliveries;

-- The queue of each destination, oldest first.
CREATE INDEX idx_notification_deliveries_queue
    ON notification_deliveries(channel_id, status, id);
CREATE INDEX idx_notification_deliveries_created ON notification_deliveries(created_at);
