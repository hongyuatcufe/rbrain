-- Add source_chunk_idx to links: records which chunk of the SOURCE page emitted this link.
-- Allows per-chunk attribution filtering in build_chunk_block.
-- Requires rebuilding the table to change the UNIQUE constraint.
CREATE TABLE new_links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_slug TEXT NOT NULL REFERENCES pages(slug) ON DELETE CASCADE,
    target_slug TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    context TEXT,
    created_at TEXT NOT NULL,
    chunk_id INTEGER NOT NULL DEFAULT -1,
    source_chunk_idx INTEGER NOT NULL DEFAULT -1,
    UNIQUE(source_slug, target_slug, edge_type, chunk_id, source_chunk_idx)
) STRICT;

INSERT INTO new_links (id, source_slug, target_slug, edge_type, context, created_at, chunk_id, source_chunk_idx)
SELECT id, source_slug, target_slug, edge_type, context, created_at, chunk_id, -1 FROM links;

DROP TABLE links;
ALTER TABLE new_links RENAME TO links;

CREATE INDEX idx_links_target ON links(target_slug);
CREATE INDEX idx_links_type ON links(edge_type);
CREATE INDEX idx_links_source ON links(source_slug);
CREATE INDEX idx_links_chunk ON links(chunk_id);
CREATE INDEX idx_links_source_chunk ON links(source_slug, source_chunk_idx);
