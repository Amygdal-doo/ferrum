-- Page views counted on the server (`crate::visits`), for `/stats`.
-- One row per UTC day, page and source; no IP addresses, cookies or user agents.

CREATE TABLE site_views (
    day    date   NOT NULL,
    path   text   NOT NULL,
    -- Where the views came from: a known site (x, linkedin, …), internal,
    -- direct or other. Never the full referring URL.
    source text   NOT NULL,
    views  bigint NOT NULL,
    PRIMARY KEY (day, path, source)
);
