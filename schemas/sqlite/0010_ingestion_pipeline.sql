CREATE TABLE ingestion_batches (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    actor_user_id TEXT REFERENCES local_users(id),
    source TEXT NOT NULL CHECK (source IN ('picker', 'drop', 'watch')),
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE ingestion_items (
    id TEXT PRIMARY KEY NOT NULL,
    batch_id TEXT NOT NULL REFERENCES ingestion_batches(id),
    document_id TEXT REFERENCES documents(id),
    original_filename TEXT NOT NULL CHECK (length(trim(original_filename)) > 0),
    existing_document_id TEXT REFERENCES documents(id),
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX ingestion_items_batch_index ON ingestion_items(batch_id, created_at_ms, id);

CREATE TABLE ingestion_batch_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT UNIQUE NOT NULL,
    batch_id TEXT NOT NULL REFERENCES ingestion_batches(id),
    state TEXT NOT NULL CHECK (state IN (
        'queued', 'running', 'paused', 'cancel_requested', 'cancelled',
        'completed', 'completed_with_errors'
    )),
    reason TEXT NOT NULL,
    occurred_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX ingestion_batch_events_current_index
ON ingestion_batch_events(batch_id, sequence DESC);

CREATE TABLE ingestion_item_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT UNIQUE NOT NULL,
    item_id TEXT NOT NULL REFERENCES ingestion_items(id),
    state TEXT NOT NULL CHECK (state IN (
        'queued', 'processing', 'review_ready', 'probable_duplicate',
        'duplicate_linked', 'duplicate_rejected', 'failed', 'cancelled'
    )),
    attempt_number INTEGER NOT NULL CHECK (attempt_number >= 0),
    error_code TEXT,
    message TEXT NOT NULL,
    probable_duplicate_document_id TEXT REFERENCES documents(id),
    matched_fields_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(matched_fields_json)),
    occurred_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX ingestion_item_events_current_index
ON ingestion_item_events(item_id, sequence DESC);

CREATE TABLE watched_folder_config_versions (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT UNIQUE NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    actor_user_id TEXT REFERENCES local_users(id),
    folder_path TEXT,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    stable_seconds INTEGER NOT NULL CHECK (stable_seconds BETWEEN 2 AND 300),
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX watched_folder_config_current_index
ON watched_folder_config_versions(workspace_id, sequence DESC);

CREATE TABLE watched_file_receipts (
    id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    fingerprint_sha256 TEXT NOT NULL CHECK (length(fingerprint_sha256) = 64),
    batch_id TEXT NOT NULL REFERENCES ingestion_batches(id),
    observed_at_ms INTEGER NOT NULL,
    UNIQUE(workspace_id, fingerprint_sha256)
) STRICT;

CREATE TABLE supplier_template_versions (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT UNIQUE NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    supplier_key TEXT NOT NULL CHECK (length(trim(supplier_key)) > 0),
    field_name TEXT NOT NULL CHECK (field_name IN (
        'invoice_number', 'invoice_date', 'supplier_name', 'total_amount'
    )),
    label_text TEXT NOT NULL CHECK (length(trim(label_text)) BETWEEN 2 AND 80),
    status TEXT NOT NULL CHECK (status IN ('proposed', 'approved', 'retired')),
    based_on_correction_count INTEGER NOT NULL CHECK (based_on_correction_count >= 2),
    supersedes_id TEXT REFERENCES supplier_template_versions(id),
    actor_user_id TEXT REFERENCES local_users(id),
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX supplier_template_current_index
ON supplier_template_versions(workspace_id, supplier_key, field_name, sequence DESC);

CREATE TRIGGER ingestion_batches_are_append_only_update BEFORE UPDATE ON ingestion_batches
BEGIN SELECT RAISE(ABORT, 'ingestion batches are append-only'); END;
CREATE TRIGGER ingestion_batches_are_append_only_delete BEFORE DELETE ON ingestion_batches
BEGIN SELECT RAISE(ABORT, 'ingestion batches are append-only'); END;
CREATE TRIGGER ingestion_items_are_append_only_update BEFORE UPDATE ON ingestion_items
BEGIN SELECT RAISE(ABORT, 'ingestion items are append-only'); END;
CREATE TRIGGER ingestion_items_are_append_only_delete BEFORE DELETE ON ingestion_items
BEGIN SELECT RAISE(ABORT, 'ingestion items are append-only'); END;
CREATE TRIGGER ingestion_batch_events_are_append_only_update BEFORE UPDATE ON ingestion_batch_events
BEGIN SELECT RAISE(ABORT, 'ingestion batch events are append-only'); END;
CREATE TRIGGER ingestion_batch_events_are_append_only_delete BEFORE DELETE ON ingestion_batch_events
BEGIN SELECT RAISE(ABORT, 'ingestion batch events are append-only'); END;
CREATE TRIGGER ingestion_item_events_are_append_only_update BEFORE UPDATE ON ingestion_item_events
BEGIN SELECT RAISE(ABORT, 'ingestion item events are append-only'); END;
CREATE TRIGGER ingestion_item_events_are_append_only_delete BEFORE DELETE ON ingestion_item_events
BEGIN SELECT RAISE(ABORT, 'ingestion item events are append-only'); END;
CREATE TRIGGER watched_folder_config_versions_are_append_only_update BEFORE UPDATE ON watched_folder_config_versions
BEGIN SELECT RAISE(ABORT, 'watched folder configuration is append-only'); END;
CREATE TRIGGER watched_folder_config_versions_are_append_only_delete BEFORE DELETE ON watched_folder_config_versions
BEGIN SELECT RAISE(ABORT, 'watched folder configuration is append-only'); END;
CREATE TRIGGER watched_file_receipts_are_append_only_update BEFORE UPDATE ON watched_file_receipts
BEGIN SELECT RAISE(ABORT, 'watched file receipts are append-only'); END;
CREATE TRIGGER watched_file_receipts_are_append_only_delete BEFORE DELETE ON watched_file_receipts
BEGIN SELECT RAISE(ABORT, 'watched file receipts are append-only'); END;
CREATE TRIGGER supplier_template_versions_are_append_only_update BEFORE UPDATE ON supplier_template_versions
BEGIN SELECT RAISE(ABORT, 'supplier templates are append-only'); END;
CREATE TRIGGER supplier_template_versions_are_append_only_delete BEFORE DELETE ON supplier_template_versions
BEGIN SELECT RAISE(ABORT, 'supplier templates are append-only'); END;
