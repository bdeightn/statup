-- The people told by email about what the page publishes: a member, at the
-- address of their account, or a visitor, at an address they confirmed.
-- Each ticks the kinds of events they receive. The unsubscribe secret is
-- kept as it is, since every email carries it, and it can only end its own
-- row. `confirmed_at` is the latest consent, `created_at` the first.
CREATE TABLE subscribers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    email TEXT COLLATE NOCASE UNIQUE,
    locale TEXT NOT NULL,
    on_incidents INTEGER NOT NULL DEFAULT 1 CHECK (on_incidents IN (0, 1)),
    on_maintenances INTEGER NOT NULL DEFAULT 1 CHECK (on_maintenances IN (0, 1)),
    on_publications INTEGER NOT NULL DEFAULT 1 CHECK (on_publications IN (0, 1)),
    unsubscribe_secret TEXT NOT NULL,
    confirmed_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK ((user_id IS NULL) <> (email IS NULL)),
    CHECK (on_incidents + on_maintenances + on_publications > 0)
);

-- A visitor's subscription until they open the link sent to the address.
-- Only the secret's hash is kept, and only the newest request of an address
-- confirms it, for a day. Rows stay a day so the requests of the last hour
-- can be counted.
CREATE TABLE subscription_requests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    email TEXT NOT NULL COLLATE NOCASE,
    secret_hash TEXT NOT NULL,
    locale TEXT NOT NULL,
    on_incidents INTEGER NOT NULL CHECK (on_incidents IN (0, 1)),
    on_maintenances INTEGER NOT NULL CHECK (on_maintenances IN (0, 1)),
    on_publications INTEGER NOT NULL CHECK (on_publications IN (0, 1)),
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (on_incidents + on_maintenances + on_publications > 0)
);

CREATE INDEX idx_subscription_requests_email ON subscription_requests(email, created_at);
CREATE INDEX idx_subscription_requests_created ON subscription_requests(created_at);

-- One email for one subscriber, sent in the background in the order it was
-- queued. Only what people publish is sent: never a detected outage nor a
-- test. A message the subscriber may not see when its turn comes is
-- skipped, not sent.
CREATE TABLE subscriber_deliveries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    subscriber_id INTEGER NOT NULL REFERENCES subscribers(id) ON DELETE CASCADE,
    happening TEXT NOT NULL CHECK (happening IN
        ('opened', 'updated', 'rescheduled', 'started', 'closed', 'published', 'reviewed')),
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    update_id INTEGER REFERENCES event_updates(id) ON DELETE CASCADE,
    lifecycle TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'sent', 'failed', 'skipped')),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT NOT NULL DEFAULT (datetime('now')),
    failure TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    sent_at TEXT
);

CREATE INDEX idx_subscriber_deliveries_queue
    ON subscriber_deliveries(subscriber_id, status, id);
CREATE INDEX idx_subscriber_deliveries_due
    ON subscriber_deliveries(next_attempt_at, id) WHERE status = 'pending';
CREATE INDEX idx_subscriber_deliveries_created ON subscriber_deliveries(created_at);
CREATE INDEX idx_subscriber_deliveries_waiting
    ON subscriber_deliveries(created_at) WHERE status = 'pending';
CREATE INDEX idx_subscriber_deliveries_event ON subscriber_deliveries(event_id);
CREATE INDEX idx_subscriber_deliveries_update ON subscriber_deliveries(update_id);
