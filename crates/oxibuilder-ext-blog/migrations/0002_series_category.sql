-- 시리즈 + 카테고리 (doc/02 §2.6 확장)
ALTER TABLE blog_post ADD COLUMN category TEXT;
ALTER TABLE blog_post ADD COLUMN series_id INTEGER REFERENCES blog_series(id) ON DELETE SET NULL;
ALTER TABLE blog_post ADD COLUMN series_order INTEGER;

CREATE TABLE IF NOT EXISTS blog_series (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    slug TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
