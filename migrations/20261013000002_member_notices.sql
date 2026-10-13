-- What the app tells a publisher or an administrator, listed under the bell
-- of the masthead: one row per member, read on their own.
CREATE TABLE member_notices (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind TEXT NOT NULL
        CHECK (kind IN ('update_overdue', 'outage_detected', 'delivery_failed')),
    event_id INTEGER REFERENCES events(id) ON DELETE CASCADE,
    service_id INTEGER REFERENCES services(id) ON DELETE CASCADE,
    channel_id INTEGER REFERENCES notification_channels(id) ON DELETE CASCADE,
    -- When the overdue update was announced for.
    due_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    read_at TEXT
);

CREATE INDEX idx_member_notices_user ON member_notices(user_id, created_at);
CREATE INDEX idx_member_notices_unread ON member_notices(user_id) WHERE read_at IS NULL;

-- Who announced an incident's next update, reminded once it is overdue.
ALTER TABLE events ADD COLUMN next_update_by INTEGER REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE events ADD COLUMN next_update_reminded INTEGER NOT NULL DEFAULT 0;

-- The minute round looks for the announced updates not yet reminded.
CREATE INDEX idx_events_update_reminders ON events(next_update_at)
    WHERE next_update_at IS NOT NULL AND next_update_reminded = 0;
