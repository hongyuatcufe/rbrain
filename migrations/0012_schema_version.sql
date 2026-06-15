ALTER TABLE pages ADD COLUMN schema_version INTEGER NOT NULL DEFAULT 1;

CREATE INDEX idx_pages_schema_version ON pages(schema_version);
