-- A notice may stand for a wave: the services that fell with the one it
-- names, counted.
ALTER TABLE member_notices ADD COLUMN others INTEGER NOT NULL DEFAULT 0;
