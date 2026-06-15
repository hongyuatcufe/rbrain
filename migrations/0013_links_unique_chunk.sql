CREATE UNIQUE INDEX IF NOT EXISTS idx_links_unique_chunk
ON links(source_slug, target_slug, edge_type, chunk_id);
