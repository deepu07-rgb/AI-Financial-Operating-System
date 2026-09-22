CREATE TABLE party_versions (
    party_id TEXT NOT NULL REFERENCES parties(id),
    version INTEGER NOT NULL CHECK (version >= 1),
    kind TEXT NOT NULL CHECK (kind IN ('vendor', 'customer', 'both')),
    legal_name TEXT NOT NULL CHECK (length(trim(legal_name)) BETWEEN 1 AND 240),
    tax_identifier TEXT,
    payment_terms_days INTEGER CHECK (payment_terms_days IS NULL OR payment_terms_days BETWEEN 0 AND 3650),
    tax_label TEXT,
    actor_user_id TEXT REFERENCES local_users(id),
    reason TEXT NOT NULL CHECK (length(trim(reason)) BETWEEN 1 AND 500),
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (party_id, version)
) STRICT;

INSERT INTO party_versions(
    party_id, version, kind, legal_name, tax_identifier, payment_terms_days,
    tax_label, actor_user_id, reason, created_at_ms
)
SELECT id, 1, kind, legal_name, tax_identifier, NULL, NULL, NULL,
       'Existing party entered versioned master', created_at_ms
FROM parties;

CREATE TABLE party_merge_events (
    id TEXT PRIMARY KEY NOT NULL,
    source_party_id TEXT NOT NULL REFERENCES parties(id),
    target_party_id TEXT NOT NULL REFERENCES parties(id),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    reason TEXT NOT NULL CHECK (length(trim(reason)) BETWEEN 1 AND 500),
    occurred_at_ms INTEGER NOT NULL,
    CHECK (source_party_id <> target_party_id)
) STRICT;

CREATE UNIQUE INDEX party_merge_source_once
ON party_merge_events(source_party_id);

CREATE TABLE vendor_bank_fingerprints (
    id TEXT PRIMARY KEY NOT NULL,
    party_id TEXT NOT NULL REFERENCES parties(id),
    fingerprint_sha256 TEXT NOT NULL CHECK (
        length(fingerprint_sha256) = 64 AND fingerprint_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    masked_label TEXT NOT NULL CHECK (length(trim(masked_label)) BETWEEN 1 AND 120),
    status TEXT NOT NULL CHECK (status IN ('trusted', 'revoked')),
    evidence_document_id TEXT REFERENCES documents(id),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    reason TEXT NOT NULL CHECK (length(trim(reason)) BETWEEN 1 AND 500),
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX vendor_bank_fingerprint_current
ON vendor_bank_fingerprints(party_id, created_at_ms DESC);

CREATE TABLE operational_categories (
    id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version >= 1),
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 120),
    active INTEGER NOT NULL CHECK (active IN (0, 1)),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    reason TEXT NOT NULL CHECK (length(trim(reason)) BETWEEN 1 AND 500),
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (id, version)
) STRICT;

CREATE TABLE invoice_operational_versions (
    invoice_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version >= 1),
    direction TEXT NOT NULL CHECK (direction IN ('payable', 'receivable')),
    category_id TEXT,
    po_number TEXT CHECK (po_number IS NULL OR length(trim(po_number)) BETWEEN 1 AND 120),
    grn_number TEXT CHECK (grn_number IS NULL OR length(trim(grn_number)) BETWEEN 1 AND 120),
    actor_user_id TEXT REFERENCES local_users(id),
    reason TEXT NOT NULL CHECK (length(trim(reason)) BETWEEN 1 AND 500),
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (invoice_id, version)
) STRICT;

INSERT INTO invoice_operational_versions(
    invoice_id, version, direction, category_id, po_number, grn_number,
    actor_user_id, reason, created_at_ms
)
SELECT invoice.id, 1,
       CASE WHEN party.kind = 'customer' THEN 'receivable' ELSE 'payable' END,
       NULL, NULL, NULL, NULL, 'Existing invoice entered operational register',
       invoice.created_at_ms
FROM invoice_drafts AS invoice
JOIN (SELECT id, MAX(version) AS version FROM invoice_drafts GROUP BY id) AS latest
  ON latest.id = invoice.id AND latest.version = invoice.version
LEFT JOIN parties AS party ON party.id = invoice.party_id;

CREATE TABLE obligation_events (
    id TEXT PRIMARY KEY NOT NULL,
    invoice_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN (
        'settlement', 'write_off', 'reversal', 'dispute_opened', 'dispute_resolved'
    )),
    amount_minor_units INTEGER NOT NULL CHECK (amount_minor_units >= 0),
    currency TEXT NOT NULL CHECK (
        length(currency) = 3 AND currency GLOB '[A-Z][A-Z][A-Z]'
    ),
    effective_date TEXT NOT NULL CHECK (length(effective_date) = 10),
    note TEXT NOT NULL CHECK (length(trim(note)) BETWEEN 1 AND 500),
    source TEXT NOT NULL CHECK (source IN ('manual', 'csv', 'tally_xml', 'bulk_undo')),
    external_key_sha256 TEXT CHECK (
        external_key_sha256 IS NULL OR (
            length(external_key_sha256) = 64 AND external_key_sha256 NOT GLOB '*[^0-9a-f]*'
        )
    ),
    reverses_event_id TEXT REFERENCES obligation_events(id),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    occurred_at_ms INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX obligation_import_idempotency
ON obligation_events(source, external_key_sha256)
WHERE external_key_sha256 IS NOT NULL;

CREATE INDEX obligation_invoice_timeline
ON obligation_events(invoice_id, occurred_at_ms, id);

CREATE TABLE settlement_import_runs (
    id TEXT PRIMARY KEY NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('csv', 'tally_xml')),
    source_filename TEXT NOT NULL CHECK (length(trim(source_filename)) BETWEEN 1 AND 255),
    source_sha256 TEXT NOT NULL CHECK (
        length(source_sha256) = 64 AND source_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    valid_count INTEGER NOT NULL CHECK (valid_count >= 0),
    invalid_count INTEGER NOT NULL CHECK (invalid_count >= 0),
    imported_count INTEGER NOT NULL CHECK (imported_count >= 0),
    skipped_count INTEGER NOT NULL CHECK (skipped_count >= 0),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    imported_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE settlement_import_items (
    run_id TEXT NOT NULL REFERENCES settlement_import_runs(id),
    row_number INTEGER NOT NULL CHECK (row_number >= 1),
    outcome TEXT NOT NULL CHECK (outcome IN ('imported', 'skipped_duplicate', 'failed')),
    invoice_id TEXT,
    obligation_event_id TEXT REFERENCES obligation_events(id),
    errors_json TEXT NOT NULL CHECK (json_valid(errors_json) AND json_type(errors_json) = 'array'),
    PRIMARY KEY (run_id, row_number)
) STRICT;

CREATE TABLE cash_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    snapshot_date TEXT NOT NULL CHECK (length(snapshot_date) = 10),
    currency TEXT NOT NULL CHECK (
        length(currency) = 3 AND currency GLOB '[A-Z][A-Z][A-Z]'
    ),
    available_minor_units INTEGER NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('manual', 'csv', 'tally_xml')),
    note TEXT NOT NULL CHECK (length(trim(note)) BETWEEN 1 AND 500),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX cash_snapshots_latest
ON cash_snapshots(currency, snapshot_date DESC, created_at_ms DESC);

CREATE TABLE cash_scenario_buckets (
    id TEXT PRIMARY KEY NOT NULL,
    snapshot_id TEXT NOT NULL REFERENCES cash_snapshots(id),
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 120),
    kind TEXT NOT NULL CHECK (kind IN ('inflow', 'outflow', 'reserved')),
    amount_minor_units INTEGER NOT NULL CHECK (amount_minor_units >= 0),
    due_date TEXT CHECK (due_date IS NULL OR length(due_date) = 10),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE bulk_operation_runs (
    id TEXT PRIMARY KEY NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('assign_category', 'undo_assign_category')),
    preview_sha256 TEXT NOT NULL CHECK (
        length(preview_sha256) = 64 AND preview_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    reason TEXT NOT NULL CHECK (length(trim(reason)) BETWEEN 1 AND 500),
    affected_count INTEGER NOT NULL CHECK (affected_count >= 0),
    reverses_run_id TEXT REFERENCES bulk_operation_runs(id),
    occurred_at_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE bulk_operation_items (
    run_id TEXT NOT NULL REFERENCES bulk_operation_runs(id),
    invoice_id TEXT NOT NULL,
    prior_value_json TEXT NOT NULL CHECK (json_valid(prior_value_json)),
    new_value_json TEXT NOT NULL CHECK (json_valid(new_value_json)),
    PRIMARY KEY (run_id, invoice_id)
) STRICT;

CREATE TRIGGER party_versions_are_append_only_update BEFORE UPDATE ON party_versions BEGIN SELECT RAISE(ABORT, 'party versions are append-only'); END;
CREATE TRIGGER party_versions_are_append_only_delete BEFORE DELETE ON party_versions BEGIN SELECT RAISE(ABORT, 'party versions are append-only'); END;
CREATE TRIGGER party_merge_events_are_append_only_update BEFORE UPDATE ON party_merge_events BEGIN SELECT RAISE(ABORT, 'party merges are append-only'); END;
CREATE TRIGGER party_merge_events_are_append_only_delete BEFORE DELETE ON party_merge_events BEGIN SELECT RAISE(ABORT, 'party merges are append-only'); END;
CREATE TRIGGER vendor_bank_fingerprints_are_append_only_update BEFORE UPDATE ON vendor_bank_fingerprints BEGIN SELECT RAISE(ABORT, 'bank fingerprints are append-only'); END;
CREATE TRIGGER vendor_bank_fingerprints_are_append_only_delete BEFORE DELETE ON vendor_bank_fingerprints BEGIN SELECT RAISE(ABORT, 'bank fingerprints are append-only'); END;
CREATE TRIGGER operational_categories_are_append_only_update BEFORE UPDATE ON operational_categories BEGIN SELECT RAISE(ABORT, 'categories are append-only'); END;
CREATE TRIGGER operational_categories_are_append_only_delete BEFORE DELETE ON operational_categories BEGIN SELECT RAISE(ABORT, 'categories are append-only'); END;
CREATE TRIGGER invoice_operational_versions_are_append_only_update BEFORE UPDATE ON invoice_operational_versions BEGIN SELECT RAISE(ABORT, 'invoice operations are append-only'); END;
CREATE TRIGGER invoice_operational_versions_are_append_only_delete BEFORE DELETE ON invoice_operational_versions BEGIN SELECT RAISE(ABORT, 'invoice operations are append-only'); END;
CREATE TRIGGER obligation_events_are_append_only_update BEFORE UPDATE ON obligation_events BEGIN SELECT RAISE(ABORT, 'obligation events are append-only'); END;
CREATE TRIGGER obligation_events_are_append_only_delete BEFORE DELETE ON obligation_events BEGIN SELECT RAISE(ABORT, 'obligation events are append-only'); END;
CREATE TRIGGER settlement_import_runs_are_append_only_update BEFORE UPDATE ON settlement_import_runs BEGIN SELECT RAISE(ABORT, 'settlement imports are append-only'); END;
CREATE TRIGGER settlement_import_runs_are_append_only_delete BEFORE DELETE ON settlement_import_runs BEGIN SELECT RAISE(ABORT, 'settlement imports are append-only'); END;
CREATE TRIGGER settlement_import_items_are_append_only_update BEFORE UPDATE ON settlement_import_items BEGIN SELECT RAISE(ABORT, 'settlement import items are append-only'); END;
CREATE TRIGGER settlement_import_items_are_append_only_delete BEFORE DELETE ON settlement_import_items BEGIN SELECT RAISE(ABORT, 'settlement import items are append-only'); END;
CREATE TRIGGER cash_snapshots_are_append_only_update BEFORE UPDATE ON cash_snapshots BEGIN SELECT RAISE(ABORT, 'cash snapshots are append-only'); END;
CREATE TRIGGER cash_snapshots_are_append_only_delete BEFORE DELETE ON cash_snapshots BEGIN SELECT RAISE(ABORT, 'cash snapshots are append-only'); END;
CREATE TRIGGER cash_scenario_buckets_are_append_only_update BEFORE UPDATE ON cash_scenario_buckets BEGIN SELECT RAISE(ABORT, 'cash buckets are append-only'); END;
CREATE TRIGGER cash_scenario_buckets_are_append_only_delete BEFORE DELETE ON cash_scenario_buckets BEGIN SELECT RAISE(ABORT, 'cash buckets are append-only'); END;
CREATE TRIGGER bulk_operation_runs_are_append_only_update BEFORE UPDATE ON bulk_operation_runs BEGIN SELECT RAISE(ABORT, 'bulk runs are append-only'); END;
CREATE TRIGGER bulk_operation_runs_are_append_only_delete BEFORE DELETE ON bulk_operation_runs BEGIN SELECT RAISE(ABORT, 'bulk runs are append-only'); END;
CREATE TRIGGER bulk_operation_items_are_append_only_update BEFORE UPDATE ON bulk_operation_items BEGIN SELECT RAISE(ABORT, 'bulk items are append-only'); END;
CREATE TRIGGER bulk_operation_items_are_append_only_delete BEFORE DELETE ON bulk_operation_items BEGIN SELECT RAISE(ABORT, 'bulk items are append-only'); END;
