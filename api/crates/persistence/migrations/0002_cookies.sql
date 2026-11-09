-- Cookie jar: cookies received in Set-Cookie headers, one row per
-- (domain, path, name) as RFC 6265 identifies them. Values may be session
-- tokens; like the rest of this file they stay local and are never logged.

CREATE TABLE cookies (
    domain     TEXT NOT NULL,
    path       TEXT NOT NULL,
    name       TEXT NOT NULL,
    data       TEXT NOT NULL,             -- JSON StoredCookie
    updated_at TEXT NOT NULL,
    PRIMARY KEY (domain, path, name)
);
