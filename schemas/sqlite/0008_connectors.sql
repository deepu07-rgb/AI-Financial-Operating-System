CREATE TABLE connector_import_runs (
    id TEXT PRIMARY KEY NOT NULL,
    connector_key TEXT NOT NULL CHECK (connector_key IN ('csv', 'tally_xml')),
    connector_version INTEGER NOT NULL CHECK (connector_version >= 1),
    sdk_version INTEGER NOT NULL CHECK (sdk_version >= 1),
    entity_kind TEXT NOT NULL CHECK (entity_kind IN ('party', 'invoice')),
    source_filename TEXT NOT NULL CHECK (length(trim(source_filename)) BETWEEN 1 AND 255),
    source_sha256 TEXT NOT NULL CHECK (
        length(source_sha256) = 64 AND source_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    mapping_json TEXT NOT NULL CHECK (json_valid(mapping_json)),
    mapping_sha256 TEXT NOT NULL CHECK (
        length(mapping_sha256) = 64 AND mapping_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    valid_count INTEGER NOT NULL CHECK (valid_count >= 0),
    invalid_count INTEGER NOT NULL CHECK (invalid_count >= 0),
    imported_count INTEGER NOT NULL CHECK (imported_count >= 0),
    skipped_count INTEGER NOT NULL CHECK (skipped_count >= 0),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    imported_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX connector_import_runs_created_index
ON connector_import_runs(imported_at_ms DESC, connector_key);

CREATE TRIGGER connector_import_runs_are_append_only_update
BEFORE UPDATE ON connector_import_runs
BEGIN
    SELECT RAISE(ABORT, 'connector import runs are append-only');
END;

CREATE TRIGGER connector_import_runs_are_append_only_delete
BEFORE DELETE ON connector_import_runs
BEGIN
    SELECT RAISE(ABORT, 'connector import runs are append-only');
END;

CREATE TABLE connector_entity_links (
    connector_key TEXT NOT NULL,
    connector_version INTEGER NOT NULL CHECK (connector_version >= 1),
    entity_kind TEXT NOT NULL CHECK (entity_kind IN ('party', 'invoice')),
    external_key TEXT NOT NULL CHECK (length(trim(external_key)) BETWEEN 1 AND 240),
    external_key_sha256 TEXT NOT NULL CHECK (
        length(external_key_sha256) = 64 AND external_key_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    local_entity_id TEXT NOT NULL,
    first_run_id TEXT NOT NULL REFERENCES connector_import_runs(id),
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (connector_key, connector_version, entity_kind, external_key_sha256),
    UNIQUE (connector_key, connector_version, entity_kind, local_entity_id)
) STRICT;

CREATE TRIGGER connector_entity_links_are_append_only_update
BEFORE UPDATE ON connector_entity_links
BEGIN
    SELECT RAISE(ABORT, 'connector entity links are append-only');
END;

CREATE TRIGGER connector_entity_links_are_append_only_delete
BEFORE DELETE ON connector_entity_links
BEGIN
    SELECT RAISE(ABORT, 'connector entity links are append-only');
END;

CREATE TABLE external_invoice_staging (
    id TEXT PRIMARY KEY NOT NULL,
    connector_key TEXT NOT NULL,
    connector_version INTEGER NOT NULL CHECK (connector_version >= 1),
    external_id TEXT NOT NULL CHECK (length(trim(external_id)) BETWEEN 1 AND 240),
    party_id TEXT REFERENCES parties(id),
    party_external_id TEXT,
    party_name TEXT NOT NULL CHECK (length(trim(party_name)) BETWEEN 1 AND 240),
    invoice_number TEXT NOT NULL CHECK (length(trim(invoice_number)) BETWEEN 1 AND 240),
    invoice_date TEXT NOT NULL CHECK (length(invoice_date) = 10),
    due_date TEXT CHECK (due_date IS NULL OR length(due_date) = 10),
    total_minor_units INTEGER NOT NULL CHECK (total_minor_units >= 0),
    currency TEXT NOT NULL CHECK (
        length(currency) = 3 AND currency GLOB '[A-Z][A-Z][A-Z]'
    ),
    direction TEXT NOT NULL CHECK (direction IN ('payable', 'receivable')),
    status TEXT NOT NULL CHECK (status = 'needs_review'),
    import_run_id TEXT NOT NULL REFERENCES connector_import_runs(id),
    imported_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX external_invoice_staging_review_index
ON external_invoice_staging(status, imported_at_ms DESC);

CREATE TRIGGER external_invoice_staging_are_append_only_update
BEFORE UPDATE ON external_invoice_staging
BEGIN
    SELECT RAISE(ABORT, 'external invoice staging is append-only');
END;

CREATE TRIGGER external_invoice_staging_are_append_only_delete
BEFORE DELETE ON external_invoice_staging
BEGIN
    SELECT RAISE(ABORT, 'external invoice staging is append-only');
END;

CREATE TABLE connector_import_items (
    run_id TEXT NOT NULL REFERENCES connector_import_runs(id),
    row_number INTEGER NOT NULL CHECK (row_number >= 1),
    entity_kind TEXT NOT NULL CHECK (entity_kind IN ('party', 'invoice')),
    outcome TEXT NOT NULL CHECK (outcome IN ('imported', 'skipped_duplicate', 'failed')),
    local_entity_id TEXT,
    canonical_sha256 TEXT CHECK (
        canonical_sha256 IS NULL OR (
            length(canonical_sha256) = 64 AND canonical_sha256 NOT GLOB '*[^0-9a-f]*'
        )
    ),
    errors_json TEXT NOT NULL CHECK (
        json_valid(errors_json) AND json_type(errors_json) = 'array'
    ),
    PRIMARY KEY (run_id, row_number)
) STRICT;

CREATE TRIGGER connector_import_items_are_append_only_update
BEFORE UPDATE ON connector_import_items
BEGIN
    SELECT RAISE(ABORT, 'connector import items are append-only');
END;

CREATE TRIGGER connector_import_items_are_append_only_delete
BEFORE DELETE ON connector_import_items
BEGIN
    SELECT RAISE(ABORT, 'connector import items are append-only');
END;

CREATE TABLE connector_exports (
    id TEXT PRIMARY KEY NOT NULL,
    connector_key TEXT NOT NULL CHECK (connector_key = 'csv'),
    connector_version INTEGER NOT NULL CHECK (connector_version >= 1),
    entity_kind TEXT NOT NULL CHECK (entity_kind IN ('party', 'invoice')),
    filename TEXT NOT NULL CHECK (length(trim(filename)) BETWEEN 1 AND 255),
    sha256 TEXT NOT NULL CHECK (
        length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    byte_size INTEGER NOT NULL CHECK (byte_size > 0),
    record_count INTEGER NOT NULL CHECK (record_count >= 0),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    exported_at_ms INTEGER NOT NULL
) STRICT;

CREATE TRIGGER connector_exports_are_append_only_update
BEFORE UPDATE ON connector_exports
BEGIN
    SELECT RAISE(ABORT, 'connector exports are append-only');
END;

CREATE TRIGGER connector_exports_are_append_only_delete
BEFORE DELETE ON connector_exports
BEGIN
    SELECT RAISE(ABORT, 'connector exports are append-only');
END;
