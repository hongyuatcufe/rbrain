-- Create new_links table
CREATE TABLE new_links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_slug TEXT NOT NULL REFERENCES pages(slug) ON DELETE CASCADE,
    target_slug TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    context TEXT,
    created_at TEXT NOT NULL,
    chunk_id INTEGER NOT NULL DEFAULT -1,
    UNIQUE(source_slug, target_slug, edge_type, chunk_id)
) STRICT;

-- Copy data
INSERT INTO new_links (id, source_slug, target_slug, edge_type, context, created_at, chunk_id)
SELECT id, source_slug, target_slug, edge_type, context, created_at, -1 FROM links;

-- Drop old table
DROP TABLE links;

-- Rename table
ALTER TABLE new_links RENAME TO links;

-- Re-create indexes
CREATE INDEX idx_links_target ON links(target_slug);
CREATE INDEX idx_links_type ON links(edge_type);
CREATE INDEX idx_links_source ON links(source_slug);
CREATE INDEX idx_links_chunk ON links(chunk_id);
