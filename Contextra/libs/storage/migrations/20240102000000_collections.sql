-- Create the collections table.
-- documents.collection_id remains a bare UUID (no FK added) to avoid
-- breaking existing rows that were ingested before this migration.
CREATE TABLE IF NOT EXISTS collections (
    id       UUID    PRIMARY KEY,
    name     TEXT    NOT NULL,
    metadata JSONB   NOT NULL DEFAULT '{}'
);

-- Extend conversations with optional title and metadata so the gateway
-- REST API can round-trip those fields.  Both additions are safe:
--   * ADD COLUMN IF NOT EXISTS is idempotent.
--   * title is nullable  → existing rows get NULL (shown as null in JSON).
--   * metadata has a DEFAULT → existing rows get '{}'.
ALTER TABLE conversations
    ADD COLUMN IF NOT EXISTS title    TEXT,
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}';
