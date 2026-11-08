-- Ferrum local application state. Target-database data never lives here,
-- and neither do secrets (those are in the OS credential vault).

CREATE TABLE settings (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL,            -- JSON
    updated_at TEXT NOT NULL
);

CREATE TABLE connection_profiles (
    id         TEXT PRIMARY KEY NOT NULL, -- UUID
    data       TEXT NOT NULL,             -- JSON ConnectionProfile (no secrets)
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE editor_tabs (
    id         TEXT PRIMARY KEY NOT NULL, -- UUID
    title      TEXT NOT NULL,
    sql        TEXT NOT NULL,
    profile_id TEXT,
    position   INTEGER NOT NULL,
    updated_at TEXT NOT NULL
);
