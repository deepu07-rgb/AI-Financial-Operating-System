CREATE TABLE report_templates (
    id TEXT NOT NULL,
    template_key TEXT NOT NULL CHECK (length(trim(template_key)) BETWEEN 1 AND 80),
    version INTEGER NOT NULL CHECK (version >= 1),
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 160),
    definition_json TEXT NOT NULL CHECK (json_valid(definition_json)),
    implementation_status TEXT NOT NULL CHECK (
        implementation_status IN ('active', 'partial_source_coverage', 'awaiting_input')
    ),
    actor_user_id TEXT REFERENCES local_users(id),
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (id, version),
    UNIQUE (template_key, version)
) STRICT;

CREATE INDEX report_templates_latest_index
ON report_templates(template_key, version DESC);

CREATE TRIGGER report_templates_are_append_only_update
BEFORE UPDATE ON report_templates
BEGIN
    SELECT RAISE(ABORT, 'report templates are append-only');
END;

CREATE TRIGGER report_templates_are_append_only_delete
BEFORE DELETE ON report_templates
BEGIN
    SELECT RAISE(ABORT, 'report templates are append-only');
END;

INSERT INTO report_templates(
    id, template_key, version, name, definition_json,
    implementation_status, actor_user_id, created_at_ms
) VALUES
    ('00000000-0000-0000-0000-000000000201', 'RPT-001', 1, 'Payables due schedule',
     '{"dataset":"payables_due_schedule","columns":["party","invoice_number","issue_date","due_date","total","currency","confidence","document_id"]}',
     'partial_source_coverage', NULL, 0),
    ('00000000-0000-0000-0000-000000000202', 'RPT-002', 1, 'Receivables due schedule',
     '{"dataset":"receivables_due_schedule","columns":["party","invoice_number","issue_date","due_date","total","currency","confidence","document_id"]}',
     'partial_source_coverage', NULL, 0),
    ('00000000-0000-0000-0000-000000000203', 'RPT-003', 1, 'Invoice register',
     '{"dataset":"invoice_register","columns":["invoice_number","direction","party","issue_date","due_date","total","currency","status","confidence","document_id"]}',
     'active', NULL, 0),
    ('00000000-0000-0000-0000-000000000204', 'RPT-004', 1, 'Supplier price trend',
     '{"dataset":"supplier_price_trend","columns":["party","issue_date","category","total","currency","document_id"]}',
     'active', NULL, 0),
    ('00000000-0000-0000-0000-000000000205', 'RPT-005', 1, 'Exception report',
     '{"dataset":"exceptions","columns":["rule_id","severity","subject_id","status","occurred_at"]}',
     'active', NULL, 0),
    ('00000000-0000-0000-0000-000000000206', 'RPT-006', 1, 'Cash obligation calendar',
     '{"dataset":"cash_obligation_calendar","columns":[]}',
     'awaiting_input', NULL, 0),
    ('00000000-0000-0000-0000-000000000207', 'RPT-007', 1, 'Vendor summary',
     '{"dataset":"vendor_summary","columns":["party","total","currency","invoice_count","exception_count"]}',
     'active', NULL, 0),
    ('00000000-0000-0000-0000-000000000208', 'RPT-008', 1, 'Audit pack',
     '{"dataset":"audit_pack","columns":["invoice_number","party","total","currency","status","document_id"]}',
     'active', NULL, 0),
    ('00000000-0000-0000-0000-000000000209', 'RPT-009', 1, 'Custom invoice table',
     '{"dataset":"custom_invoices","columns":["invoice_number","party","issue_date","total","currency","status","document_id"]}',
     'active', NULL, 0),
    ('00000000-0000-0000-0000-000000000210', 'RPT-010', 1, 'Offline management pack',
     '{"dataset":"management_pack","pages":["executive_overview","cash_and_obligations","movements","exceptions","notes"]}',
     'partial_source_coverage', NULL, 0);

CREATE TABLE report_runs (
    id TEXT PRIMARY KEY NOT NULL,
    template_id TEXT NOT NULL,
    template_version INTEGER NOT NULL CHECK (template_version >= 1),
    period_from TEXT NOT NULL CHECK (length(period_from) = 10),
    period_to TEXT NOT NULL CHECK (length(period_to) = 10),
    currency TEXT NOT NULL CHECK (
        length(currency) = 3 AND currency GLOB '[A-Z][A-Z][A-Z]'
    ),
    contract_json TEXT NOT NULL CHECK (json_valid(contract_json)),
    contract_sha256 TEXT NOT NULL CHECK (
        length(contract_sha256) = 64 AND contract_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    row_count INTEGER NOT NULL CHECK (row_count >= 0),
    generator TEXT NOT NULL CHECK (length(trim(generator)) > 0),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    generated_at_ms INTEGER NOT NULL,
    FOREIGN KEY (template_id, template_version) REFERENCES report_templates(id, version)
) STRICT;

CREATE INDEX report_runs_generated_index
ON report_runs(generated_at_ms DESC, template_id);

CREATE TRIGGER report_runs_are_append_only_update
BEFORE UPDATE ON report_runs
BEGIN
    SELECT RAISE(ABORT, 'report runs are append-only');
END;

CREATE TRIGGER report_runs_are_append_only_delete
BEFORE DELETE ON report_runs
BEGIN
    SELECT RAISE(ABORT, 'report runs are append-only');
END;

CREATE TABLE report_exports (
    id TEXT PRIMARY KEY NOT NULL,
    report_run_id TEXT NOT NULL REFERENCES report_runs(id),
    format TEXT NOT NULL CHECK (format IN ('csv', 'pdf')),
    filename TEXT NOT NULL CHECK (length(trim(filename)) BETWEEN 1 AND 255),
    sha256 TEXT NOT NULL CHECK (
        length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    byte_size INTEGER NOT NULL CHECK (byte_size > 0),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    exported_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX report_exports_run_index
ON report_exports(report_run_id, exported_at_ms DESC);

CREATE TRIGGER report_exports_are_append_only_update
BEFORE UPDATE ON report_exports
BEGIN
    SELECT RAISE(ABORT, 'report exports are append-only');
END;

CREATE TRIGGER report_exports_are_append_only_delete
BEFORE DELETE ON report_exports
BEGIN
    SELECT RAISE(ABORT, 'report exports are append-only');
END;
