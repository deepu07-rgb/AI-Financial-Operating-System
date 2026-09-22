ALTER TABLE documents
ADD COLUMN byte_size INTEGER NOT NULL DEFAULT 0 CHECK (byte_size >= 0);

ALTER TABLE documents
ADD COLUMN media_type TEXT NOT NULL DEFAULT 'application/octet-stream' CHECK (
    media_type IN ('application/pdf', 'image/jpeg', 'image/png', 'application/octet-stream')
);

CREATE INDEX documents_hash_and_time_index
ON documents(sha256, imported_at_ms, id);
