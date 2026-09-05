CREATE TABLE IF NOT EXISTS conversations (
    id UUID PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS messages (
    id UUID PRIMARY KEY,
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role VARCHAR(50) NOT NULL,
    content TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS documents (
    id UUID PRIMARY KEY,
    collection_id UUID NOT NULL,
    content TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS chunks (
    id UUID PRIMARY KEY,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS api_keys (
    key_id VARCHAR(128) PRIMARY KEY,
    key_hash TEXT NOT NULL,
    user_id UUID NOT NULL,
    org_id UUID NOT NULL,
    scopes JSONB NOT NULL DEFAULT '[]'
);
