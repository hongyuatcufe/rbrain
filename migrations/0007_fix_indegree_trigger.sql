-- Migration 0007: Fix indegree trigger — original COUNT(*) without GROUP BY returns
-- (NULL, 0) when the JOIN finds no matching pages, violating NOT NULL on page_stats.slug.
-- Rewrite to use NEW.target_slug directly with an existence check.

DROP TRIGGER IF EXISTS update_indegree_insert;

CREATE TRIGGER IF NOT EXISTS update_indegree_insert AFTER INSERT ON links
BEGIN
    -- Ensure a stats row exists for the target page (only if that page exists)
    INSERT OR IGNORE INTO page_stats (slug, indegree)
    SELECT NEW.target_slug, 0
    FROM pages
    WHERE slug = NEW.target_slug;

    -- Update the cached indegree count
    UPDATE page_stats
    SET indegree = (SELECT COUNT(*) FROM links WHERE target_slug = NEW.target_slug)
    WHERE slug = NEW.target_slug;
END;
