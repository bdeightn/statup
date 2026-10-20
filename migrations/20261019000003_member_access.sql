-- Access by member: a restricted service or folder can be opened to single
-- members as well as to groups. Services that follow a folder carry a copy.
CREATE TABLE service_member_access (
    service_id INTEGER NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (service_id, user_id)
);

CREATE INDEX idx_service_member_access_user ON service_member_access(user_id);

CREATE TABLE folder_member_access (
    folder_id INTEGER NOT NULL REFERENCES service_folders(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (folder_id, user_id)
);

CREATE INDEX idx_folder_member_access_user ON folder_member_access(user_id);
