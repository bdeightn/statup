-- What an incident taught, written once it is resolved: what happened, why,
-- and what changes so it does not come back. One per incident, kept as its
-- author wrote it and rendered when shown. The author is the member who
-- published it; a later correction keeps them and the publication time.
CREATE TABLE event_reviews (
    event_id INTEGER PRIMARY KEY REFERENCES events(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    author_id INTEGER NOT NULL REFERENCES users(id),
    published_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
