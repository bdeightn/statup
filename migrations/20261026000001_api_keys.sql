-- Keys that let a script or a monitoring tool act as an editor through the
-- API. Only a hash of each key is kept: the key itself is shown once.
CREATE TABLE api_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    -- The first characters of the key, so an administrator can tell which
    -- key a tool holds.
    prefix TEXT NOT NULL,
    secret_hash TEXT NOT NULL UNIQUE,
    created_by INTEGER NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at TEXT,
    revoked_at TEXT
);

-- The name signs what a key writes: two keys in use never share it. A
-- revoked key keeps its name for the history it signed.
CREATE UNIQUE INDEX idx_api_keys_name_in_use ON api_keys(name COLLATE NOCASE)
    WHERE revoked_at IS NULL;

-- What a key publishes is signed with its name. The author stays the
-- administrator who made the key.
ALTER TABLE events ADD COLUMN api_key_id INTEGER REFERENCES api_keys(id);
ALTER TABLE event_updates ADD COLUMN api_key_id INTEGER REFERENCES api_keys(id);
