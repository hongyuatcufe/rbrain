CREATE TABLE query_cache (
    query_hash TEXT PRIMARY KEY,
    expansions TEXT NOT NULL,
    intent TEXT NOT NULL,
    created_at TEXT NOT NULL
) STRICT;
