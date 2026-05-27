-- Distinguish links extracted from page content from explicit user-created graph edges.
ALTER TABLE links ADD COLUMN is_generated INTEGER NOT NULL DEFAULT 0;

-- Rebuilding links in prior migrations removed triggers attached to the old table.
DROP TRIGGER IF EXISTS update_indegree_insert;
DROP TRIGGER IF EXISTS update_indegree_delete;

CREATE TRIGGER update_indegree_insert AFTER INSERT ON links
BEGIN
    INSERT OR IGNORE INTO page_stats (slug, indegree)
    SELECT NEW.target_slug, 0
    FROM pages
    WHERE slug = NEW.target_slug;

    UPDATE page_stats
    SET indegree = (SELECT COUNT(*) FROM links WHERE target_slug = NEW.target_slug)
    WHERE slug = NEW.target_slug;
END;

CREATE TRIGGER update_indegree_delete AFTER DELETE ON links
BEGIN
    UPDATE page_stats
    SET indegree = (SELECT COUNT(*) FROM links WHERE target_slug = OLD.target_slug)
    WHERE slug = OLD.target_slug;
END;

INSERT OR IGNORE INTO page_stats (slug, indegree)
SELECT p.slug, COUNT(l.id)
FROM pages p
LEFT JOIN links l ON p.slug = l.target_slug
GROUP BY p.slug;

UPDATE page_stats
SET indegree = (SELECT COUNT(*) FROM links WHERE links.target_slug = page_stats.slug);
