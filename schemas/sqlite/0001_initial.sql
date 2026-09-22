CREATE TABLE workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    base_currency TEXT NOT NULL CHECK (
        length(base_currency) = 3 AND base_currency GLOB '[A-Z][A-Z][A-Z]'
    ),
    locale TEXT NOT NULL CHECK (length(trim(locale)) > 0),
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE TRIGGER one_workspace_per_database
BEFORE INSERT ON workspaces
WHEN (SELECT count(*) FROM workspaces) >= 1
BEGIN
    SELECT RAISE(ABORT, 'one workspace per database');
END;

CREATE TABLE local_users (
    id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0),
    role TEXT NOT NULL CHECK (role IN ('owner', 'finance_operator', 'reviewer')),
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE parties (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('vendor', 'customer', 'both')),
    legal_name TEXT NOT NULL CHECK (length(trim(legal_name)) > 0),
    tax_identifier TEXT,
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE party_aliases (
    party_id TEXT NOT NULL REFERENCES parties(id) ON DELETE CASCADE,
    alias TEXT NOT NULL CHECK (length(trim(alias)) > 0),
    PRIMARY KEY (party_id, alias)
) STRICT;

CREATE TABLE documents (
    id TEXT PRIMARY KEY NOT NULL,
    sha256 TEXT NOT NULL CHECK (
        length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    original_filename TEXT NOT NULL CHECK (length(trim(original_filename)) > 0),
    managed_path TEXT NOT NULL CHECK (length(trim(managed_path)) > 0),
    imported_at_ms INTEGER NOT NULL,
    replaces_document_id TEXT REFERENCES documents(id)
) STRICT;

CREATE INDEX documents_sha256_index ON documents(sha256);

CREATE TRIGGER documents_are_append_only_update
BEFORE UPDATE ON documents
BEGIN
    SELECT RAISE(ABORT, 'documents are append-only');
END;

CREATE TRIGGER documents_are_append_only_delete
BEFORE DELETE ON documents
BEGIN
    SELECT RAISE(ABORT, 'documents are append-only');
END;

CREATE TABLE invoice_drafts (
    id TEXT NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id),
    party_id TEXT REFERENCES parties(id),
    invoice_number TEXT,
    invoice_date TEXT CHECK (
        invoice_date IS NULL OR length(invoice_date) = 10
    ),
    due_date TEXT CHECK (due_date IS NULL OR length(due_date) = 10),
    total_minor_units INTEGER NOT NULL,
    currency TEXT NOT NULL CHECK (
        length(currency) = 3 AND currency GLOB '[A-Z][A-Z][A-Z]'
    ),
    status TEXT NOT NULL CHECK (status IN ('draft', 'in_review', 'approved', 'rejected')),
    version INTEGER NOT NULL CHECK (version >= 1),
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (id, version)
) STRICT;

CREATE TRIGGER invoice_snapshots_are_append_only_update
BEFORE UPDATE ON invoice_drafts
BEGIN
    SELECT RAISE(ABORT, 'invoice snapshots are append-only');
END;

CREATE TRIGGER invoice_snapshots_are_append_only_delete
BEFORE DELETE ON invoice_drafts
BEGIN
    SELECT RAISE(ABORT, 'invoice snapshots are append-only');
END;

CREATE TABLE invoice_lines (
    id TEXT PRIMARY KEY NOT NULL,
    invoice_id TEXT NOT NULL,
    invoice_version INTEGER NOT NULL CHECK (invoice_version >= 1),
    position INTEGER NOT NULL CHECK (position >= 1),
    description TEXT NOT NULL CHECK (length(trim(description)) > 0),
    quantity_coefficient INTEGER NOT NULL,
    quantity_scale INTEGER NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9),
    unit_rate_minor_units INTEGER NOT NULL,
    tax_minor_units INTEGER NOT NULL,
    total_minor_units INTEGER NOT NULL,
    currency TEXT NOT NULL CHECK (
        length(currency) = 3 AND currency GLOB '[A-Z][A-Z][A-Z]'
    ),
    FOREIGN KEY (invoice_id, invoice_version) REFERENCES invoice_drafts(id, version),
    UNIQUE (invoice_id, invoice_version, position)
) STRICT;

CREATE TRIGGER invoice_lines_are_append_only_update
BEFORE UPDATE ON invoice_lines
BEGIN
    SELECT RAISE(ABORT, 'invoice lines are append-only');
END;

CREATE TRIGGER invoice_lines_are_append_only_delete
BEFORE DELETE ON invoice_lines
BEGIN
    SELECT RAISE(ABORT, 'invoice lines are append-only');
END;

CREATE TABLE review_events (
    id TEXT PRIMARY KEY NOT NULL,
    invoice_id TEXT NOT NULL,
    invoice_version INTEGER NOT NULL CHECK (invoice_version >= 1),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    kind TEXT NOT NULL CHECK (kind IN ('correction', 'approval', 'rejection', 'comment')),
    reason TEXT NOT NULL CHECK (length(trim(reason)) > 0),
    field_name TEXT,
    prior_value_json TEXT CHECK (prior_value_json IS NULL OR json_valid(prior_value_json)),
    new_value_json TEXT CHECK (new_value_json IS NULL OR json_valid(new_value_json)),
    occurred_at_ms INTEGER NOT NULL,
    FOREIGN KEY (invoice_id, invoice_version) REFERENCES invoice_drafts(id, version)
) STRICT;

CREATE TRIGGER review_events_are_append_only_update
BEFORE UPDATE ON review_events
BEGIN
    SELECT RAISE(ABORT, 'review events are append-only');
END;

CREATE TRIGGER review_events_are_append_only_delete
BEFORE DELETE ON review_events
BEGIN
    SELECT RAISE(ABORT, 'review events are append-only');
END;

CREATE TABLE audit_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    actor_user_id TEXT REFERENCES local_users(id),
    action TEXT NOT NULL CHECK (length(trim(action)) > 0),
    entity_type TEXT NOT NULL CHECK (length(trim(entity_type)) > 0),
    entity_id TEXT NOT NULL,
    details_json TEXT NOT NULL CHECK (json_valid(details_json)),
    occurred_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX audit_events_entity_index
ON audit_events(entity_type, entity_id, sequence);

CREATE TRIGGER audit_events_are_append_only_update
BEFORE UPDATE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit events are append-only');
END;

CREATE TRIGGER audit_events_are_append_only_delete
BEFORE DELETE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit events are append-only');
END;
