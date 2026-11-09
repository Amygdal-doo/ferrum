-- Query history (IDE-013): one row per executed statement. Holds the query
-- text and a result summary, never row values or secrets.
CREATE TABLE query_history (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id    TEXT NOT NULL,         -- UUID; rows are removed with the profile
    sql           TEXT NOT NULL,
    executed_at   TEXT NOT NULL,         -- RFC 3339, UTC
    duration_ms   INTEGER NOT NULL,
    status        TEXT NOT NULL,         -- completed | failed | cancelled | timed_out
    rows_returned INTEGER,
    rows_affected INTEGER,
    error         TEXT
);

CREATE INDEX query_history_by_profile ON query_history (profile_id, id);
