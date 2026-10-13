-- An open incident may say when its next update is due. The delay it was
-- given as, if any, is offered again with the following update.
ALTER TABLE events ADD COLUMN next_update_at TEXT;
ALTER TABLE events ADD COLUMN update_every_minutes INTEGER;
