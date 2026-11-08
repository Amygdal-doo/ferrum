-- FerrumGIT local application state. No secrets are stored here: SSH key
-- passphrases live in the OS keychain; profiles hold only key *paths*.
-- Timestamps are RFC 3339 UTC strings.

CREATE TABLE settings (
    key        TEXT PRIMARY KEY NOT NULL, -- 'app' | 'workspace'
    value      TEXT NOT NULL,             -- JSON
    updated_at TEXT NOT NULL
);

CREATE TABLE repos (
    id         TEXT PRIMARY KEY NOT NULL, -- UUID
    path       TEXT NOT NULL UNIQUE,
    data       TEXT NOT NULL,             -- JSON RepoEntry
    updated_at TEXT NOT NULL
);

CREATE TABLE profiles (
    id         TEXT PRIMARY KEY NOT NULL, -- UUID
    data       TEXT NOT NULL,             -- JSON Profile
    updated_at TEXT NOT NULL
);
