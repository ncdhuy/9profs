CREATE TABLE IF NOT EXISTS assistants (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    avatar TEXT,
    rules TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    backend_agent_id TEXT,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS assistant_skill_assignments (
    assistant_id TEXT NOT NULL,
    skill_id TEXT NOT NULL,
    sort_order INTEGER NOT NULL,
    PRIMARY KEY (assistant_id, skill_id),
    FOREIGN KEY (assistant_id) REFERENCES assistants(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS assistant_skill_assignments_order_idx
    ON assistant_skill_assignments (assistant_id, sort_order);
