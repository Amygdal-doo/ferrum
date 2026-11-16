-- Anonymous usage counts from FerrumLauncher (`POST /api/ping`).
-- One row per install per UTC day; no IP addresses or user agents.

CREATE TABLE usage_pings (
    day              date        NOT NULL,
    install_id       uuid        NOT NULL,
    launcher_version text        NOT NULL,
    os               text        NOT NULL,
    arch             text        NOT NULL,
    PRIMARY KEY (day, install_id)
);

-- The apps an install had active that day (known app ids only).
CREATE TABLE usage_ping_apps (
    day        date NOT NULL,
    install_id uuid NOT NULL,
    app_id     text NOT NULL,
    version    text NOT NULL,
    PRIMARY KEY (day, install_id, app_id),
    FOREIGN KEY (day, install_id) REFERENCES usage_pings (day, install_id) ON DELETE CASCADE
);

CREATE INDEX usage_ping_apps_day ON usage_ping_apps (day);
