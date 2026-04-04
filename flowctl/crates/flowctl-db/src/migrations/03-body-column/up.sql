-- Add body column to epics and tasks for SQLite-as-single-source-of-truth.
-- Body stores the markdown content (everything after frontmatter).
ALTER TABLE epics ADD COLUMN body TEXT NOT NULL DEFAULT '';
ALTER TABLE tasks ADD COLUMN body TEXT NOT NULL DEFAULT '';
