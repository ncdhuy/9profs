CREATE TABLE IF NOT EXISTS agent_backends (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    source TEXT NOT NULL,
    kind TEXT NOT NULL,
    capabilities_json TEXT NOT NULL,
    availability TEXT NOT NULL,
    availability_reason TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    sort_order INTEGER NOT NULL DEFAULT 0,
    version TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS agent_backends_order_idx
    ON agent_backends (sort_order, id);
