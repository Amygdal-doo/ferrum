-- Mock server definitions, one row per collection: the chosen port and the
-- mock responses of its saved requests. Deleted with the collection.

CREATE TABLE mocks (
    collection_id TEXT PRIMARY KEY NOT NULL
        REFERENCES collections (id) ON DELETE CASCADE,
    data          TEXT NOT NULL,          -- JSON CollectionMocks
    updated_at    TEXT NOT NULL
);
