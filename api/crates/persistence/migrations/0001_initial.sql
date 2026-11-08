-- FerrumAPI local application state.
-- Environment variables, auth and request contents stored here may contain
-- secrets. They are local-only (this file never leaves the machine) and are
-- never logged: the persistence layer logs ids and counts only.
-- Timestamps are RFC 3339 UTC strings.

CREATE TABLE settings (
    key        TEXT PRIMARY KEY NOT NULL, -- 'app' | 'workspace'
    value      TEXT NOT NULL,             -- JSON
    updated_at TEXT NOT NULL
);

CREATE TABLE collections (
    id         TEXT PRIMARY KEY NOT NULL, -- UUID
    name       TEXT NOT NULL,
    data       TEXT NOT NULL,             -- JSON Collection (whole tree)
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- The globals environment is the row with the nil UUID; it is excluded from
-- environment listings.
CREATE TABLE environments (
    id         TEXT PRIMARY KEY NOT NULL, -- UUID
    name       TEXT NOT NULL,
    data       TEXT NOT NULL,             -- JSON Environment
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE history (
    id         TEXT PRIMARY KEY NOT NULL, -- UUID
    sent_at    TEXT NOT NULL,             -- RFC 3339 (display / debugging)
    sent_at_ms INTEGER NOT NULL,          -- Unix millis, used for ordering
    data       TEXT NOT NULL              -- JSON HistoryEntry
);

CREATE INDEX history_sent_at_idx ON history (sent_at_ms);
