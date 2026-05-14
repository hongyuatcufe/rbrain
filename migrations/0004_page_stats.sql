-- Migration 0004: page_stats table with indegree cache
CREATE TABLE IF NOT EXISTS page_stats (
    slug TEXT PRIMARY KEY REFERENCES pages(slug) ON DELETE CASCADE,
    indegree INTEGER NOT NULL DEFAULT 0
) STRICT;

-- Trigger to update indegree on link insert
CREATE TRIGGER IF NOT EXISTS update_indegree_insert AFTER INSERT ON links
BEGIN
    INSERT OR IGNORE INTO page_stats (slug, indegree)
    SELECT p.slug, COUNT(*) 
    FROM pages p 
    JOIN links l ON p.slug = l.target_slug 
    WHERE l.target_slug = NEW.target_slug;
    
    UPDATE page_stats 
    SET indegree = (SELECT COUNT(*) FROM links WHERE target_slug = slug) 
    WHERE slug = NEW.target_slug;
END;

-- Trigger to update indegree on link delete
CREATE TRIGGER IF NOT EXISTS update_indegree_delete AFTER DELETE ON links
BEGIN
    UPDATE page_stats 
    SET indegree = COALESCE((SELECT COUNT(*) FROM links WHERE target_slug = OLD.target_slug), 0) 
    WHERE slug = OLD.target_slug;
END;

-- Initialize page_stats for existing pages
INSERT OR IGNORE INTO page_stats (slug, indegree)
SELECT p.slug, COUNT(l.id) 
FROM pages p 
LEFT JOIN links l ON p.slug = l.target_slug 
GROUP BY p.slug;
