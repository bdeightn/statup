-- What members did and what the app met, read by the administrators on the
-- Journal page. Names are copied at the time of the line, so it still reads
-- right once the service, event or member is gone or renamed.
CREATE TABLE journal_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL,
    category TEXT NOT NULL
        CHECK (category IN ('members', 'access', 'monitoring', 'alerts', 'system')),
    actor_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    actor_name TEXT,
    subject TEXT,
    detail TEXT,
    ip TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_journal_entries_created ON journal_entries(created_at);
CREATE INDEX idx_journal_entries_category ON journal_entries(category, created_at);
CREATE INDEX idx_journal_entries_actor ON journal_entries(actor_id, created_at);
