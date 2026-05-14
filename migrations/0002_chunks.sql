CREATE TABLE chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    page_slug TEXT NOT NULL REFERENCES pages(slug) ON DELETE CASCADE,
    chunk_idx INTEGER NOT NULL,
    text TEXT NOT NULL,
    is_compiled_truth INTEGER NOT NULL DEFAULT 0,
    language TEXT,
    embedding BLOB,
    embedding_model TEXT,
    has_embedding INTEGER NOT NULL DEFAULT 0,
    indexed_in_vectors INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    UNIQUE(page_slug, chunk_idx)
) STRICT;

CREATE INDEX idx_chunks_page ON chunks(page_slug);
CREATE INDEX idx_chunks_needs_embed ON chunks(has_embedding) WHERE has_embedding = 0;
CREATE INDEX idx_chunks_needs_vec_index ON chunks(indexed_in_vectors) WHERE indexed_in_vectors = 0 AND has_embedding = 1;
