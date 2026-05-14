CREATE TABLE pages (
    slug TEXT PRIMARY KEY,
    page_type TEXT NOT NULL,
    title TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    frontmatter TEXT NOT NULL DEFAULT '{}',
    compiled_truth TEXT NOT NULL,
    timeline TEXT NOT NULL,
    language TEXT,
    content_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX idx_pages_type ON pages(page_type);
CREATE INDEX idx_pages_updated ON pages(updated_at);
CREATE INDEX idx_pages_lang ON pages(language);
