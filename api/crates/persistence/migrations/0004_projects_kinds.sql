-- Projects and request kinds.
--
-- Collections now belong to a project folder (absolute path); NULL means the
-- collection is shared by every project. Existing collections become shared.
--
-- Requests gained a `kind` (rest, graphql, soap, odata, trpc, grpc,
-- websocket) inside the collection / history / workspace JSON. Records
-- written before have no `kind` and load as REST, so no data is rewritten.

ALTER TABLE collections ADD COLUMN project TEXT;

CREATE INDEX collections_project_idx ON collections (project);

-- Fetched API descriptions (GraphQL introspection, WSDL operations, OData
-- $metadata, gRPC reflection), keyed by kind and endpoint URL, so the editor
-- can show them again without refetching. Never holds credentials.
CREATE TABLE schema_cache (
    kind       TEXT NOT NULL,             -- 'graphql' | 'wsdl' | 'odata' | 'grpc'
    endpoint   TEXT NOT NULL,             -- substituted URL
    data       TEXT NOT NULL,             -- JSON, shape depends on kind
    fetched_at TEXT NOT NULL,
    PRIMARY KEY (kind, endpoint)
);
