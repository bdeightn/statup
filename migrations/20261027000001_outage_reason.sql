-- Who found each outage, and why when Statup's own checks did, so the
-- service panel can say it. Outages from before keep no reason.
ALTER TABLE service_outages ADD COLUMN source TEXT NOT NULL DEFAULT 'check'
    CHECK (source IN ('check', 'report'));
ALTER TABLE service_outages ADD COLUMN reason TEXT;
-- Outages that fell together are found by when they began.
CREATE INDEX idx_service_outages_started ON service_outages(started_at);
