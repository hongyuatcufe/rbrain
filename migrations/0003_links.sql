CREATE TABLE links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_slug TEXT NOT NULL REFERENCES pages(slug) ON DELETE CASCADE,
    target_slug TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    context TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(source_slug, target_slug, edge_type)
) STRICT;
CREATE INDEX idx_links_target ON links(target_slug);
CREATE INDEX idx_links_type ON links(edge_type);
CREATE INDEX idx_links_source ON links(source_slug);
