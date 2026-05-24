-- Migration 0009: dream_metadata table to track last extracted timestamp per page
CREATE TABLE IF NOT EXISTS dream_metadata (
    slug TEXT PRIMARY KEY REFERENCES pages(slug) ON DELETE CASCADE,
    last_extracted_at TEXT NOT NULL
) STRICT;
