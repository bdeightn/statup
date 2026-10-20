-- Each service sets how long its checks may fail before it shows an outage,
-- in minutes since the checks run once a minute. Existing services keep the
-- three minutes they had.
ALTER TABLE services ADD COLUMN outage_after_minutes INTEGER NOT NULL DEFAULT 3
    CHECK (outage_after_minutes BETWEEN 1 AND 60);
