-- Groups of members, and who sees which service. A service is seen by
-- everyone, by signed-in members only, or by the members of chosen groups.
-- Existing services stay visible to everyone.
ALTER TABLE services ADD COLUMN visibility TEXT NOT NULL DEFAULT 'everyone'
    CHECK (visibility IN ('everyone', 'members', 'groups'));

CREATE TABLE member_groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE group_members (
    group_id INTEGER NOT NULL REFERENCES member_groups(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, user_id)
);

CREATE INDEX idx_group_members_user ON group_members(user_id);

CREATE TABLE service_access (
    service_id INTEGER NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    group_id INTEGER NOT NULL REFERENCES member_groups(id) ON DELETE CASCADE,
    PRIMARY KEY (service_id, group_id)
);

CREATE INDEX idx_service_access_group ON service_access(group_id);
