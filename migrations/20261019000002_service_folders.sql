-- Folders of services. A folder has its own "who sees", and the services in
-- it follow it by default: they carry a copy of it, kept in step when the
-- folder changes. Existing services stay without a folder.
CREATE TABLE service_folders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    position INTEGER NOT NULL DEFAULT 0,
    visibility TEXT NOT NULL DEFAULT 'everyone'
        CHECK (visibility IN ('everyone', 'members', 'groups')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE folder_access (
    folder_id INTEGER NOT NULL REFERENCES service_folders(id) ON DELETE CASCADE,
    group_id INTEGER NOT NULL REFERENCES member_groups(id) ON DELETE CASCADE,
    PRIMARY KEY (folder_id, group_id)
);

CREATE INDEX idx_folder_access_group ON folder_access(group_id);

ALTER TABLE services ADD COLUMN folder_id INTEGER
    REFERENCES service_folders(id) ON DELETE SET NULL;
ALTER TABLE services ADD COLUMN follows_folder INTEGER NOT NULL DEFAULT 0
    CHECK (follows_folder IN (0, 1) AND (follows_folder = 0 OR folder_id IS NOT NULL));

CREATE INDEX idx_services_folder ON services(folder_id);
