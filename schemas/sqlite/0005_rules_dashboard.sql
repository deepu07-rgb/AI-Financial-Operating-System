CREATE TABLE rule_definitions (
    id TEXT NOT NULL UNIQUE,
    rule_id TEXT NOT NULL CHECK (rule_id GLOB 'RULE-[0-9][0-9][0-9]'),
    version INTEGER NOT NULL CHECK (version >= 1),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'high')),
    config_json TEXT NOT NULL CHECK (json_valid(config_json)),
    explanation_template TEXT NOT NULL CHECK (length(trim(explanation_template)) > 0),
    implementation_status TEXT NOT NULL CHECK (
        implementation_status IN ('active', 'existing_boundary', 'awaiting_input')
    ),
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (rule_id, version)
) STRICT;

CREATE TRIGGER rule_definitions_are_append_only_update
BEFORE UPDATE ON rule_definitions
BEGIN
    SELECT RAISE(ABORT, 'rule definitions are append-only');
END;

CREATE TRIGGER rule_definitions_are_append_only_delete
BEFORE DELETE ON rule_definitions
BEGIN
    SELECT RAISE(ABORT, 'rule definitions are append-only');
END;

CREATE TABLE invoice_line_category_assignments (
    id TEXT PRIMARY KEY NOT NULL,
    invoice_line_id TEXT NOT NULL REFERENCES invoice_lines(id),
    category TEXT NOT NULL CHECK (length(trim(category)) BETWEEN 1 AND 120),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    source TEXT NOT NULL CHECK (source IN ('user', 'rule')),
    reason TEXT NOT NULL CHECK (length(trim(reason)) > 0),
    occurred_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX invoice_line_category_assignments_line_index
ON invoice_line_category_assignments(invoice_line_id, occurred_at_ms, id);

CREATE TRIGGER invoice_line_category_assignments_are_append_only_update
BEFORE UPDATE ON invoice_line_category_assignments
BEGIN
    SELECT RAISE(ABORT, 'invoice line category assignments are append-only');
END;

CREATE TRIGGER invoice_line_category_assignments_are_append_only_delete
BEFORE DELETE ON invoice_line_category_assignments
BEGIN
    SELECT RAISE(ABORT, 'invoice line category assignments are append-only');
END;

INSERT INTO rule_definitions(
    id, rule_id, version, name, enabled, severity, config_json,
    explanation_template, implementation_status, created_at_ms
) VALUES
    ('00000000-0000-0000-0000-000000000101', 'RULE-001', 1, 'Exact duplicate file', 1, 'high', '{}',
     'The managed source hash matches existing evidence.', 'existing_boundary', 0),
    ('00000000-0000-0000-0000-000000000102', 'RULE-002', 1, 'Probable duplicate invoice', 1, 'high', '{"similar_date_days":3}',
     'Vendor and invoice identity or nearby amount/date match another approved invoice.', 'active', 0),
    ('00000000-0000-0000-0000-000000000103', 'RULE-003', 1, 'Arithmetic mismatch', 1, 'high', '{"tolerance_minor_units":1}',
     'Line totals do not reconcile to the approved invoice total.', 'active', 0),
    ('00000000-0000-0000-0000-000000000104', 'RULE-004', 1, 'Price increase', 1, 'warning', '{"increase_basis_points":1000}',
     'Unit price increased above the configured historical baseline threshold.', 'active', 0),
    ('00000000-0000-0000-0000-000000000105', 'RULE-005', 1, 'Due soon', 1, 'warning', '{"window_days":14}',
     'An approved staged invoice is approaching its due date.', 'active', 0),
    ('00000000-0000-0000-0000-000000000106', 'RULE-006', 1, 'Overdue', 0, 'high', '{"requires_payment_clearance":true}',
     'Due date passed and the obligation remains unpaid or uncleared.', 'awaiting_input', 0),
    ('00000000-0000-0000-0000-000000000107', 'RULE-007', 1, 'Penalty exposure', 0, 'warning', '{"requires_confirmed_terms":true}',
     'Confirmed late-fee terms create an estimated cost of delay.', 'awaiting_input', 0),
    ('00000000-0000-0000-0000-000000000108', 'RULE-008', 1, 'Unusual amount', 1, 'warning', '{"multiplier_basis_points":20000,"minimum_history":3}',
     'Amount exceeds the configured multiple of vendor history.', 'active', 0),
    ('00000000-0000-0000-0000-000000000109', 'RULE-009', 1, 'New bank details', 0, 'high', '{"requires_confirmed_vendor_bank":true}',
     'Bank details differ from the confirmed vendor master.', 'awaiting_input', 0),
    ('00000000-0000-0000-0000-000000000110', 'RULE-010', 1, 'Missing tax identifier', 0, 'warning', '{"requires_jurisdiction_pack":true}',
     'A selected jurisdiction pack requires a missing identifier.', 'awaiting_input', 0),
    ('00000000-0000-0000-0000-000000000111', 'RULE-011', 1, 'Cash gap', 0, 'high', '{"requires_cash_scenario":true}',
     'Projected obligations exceed entered available cash.', 'awaiting_input', 0),
    ('00000000-0000-0000-0000-000000000112', 'RULE-012', 1, 'Concentration risk', 1, 'warning', '{"threshold_basis_points":3000}',
     'One vendor exceeds the configured share of approved spend.', 'active', 0);

CREATE TABLE rule_events (
    id TEXT PRIMARY KEY NOT NULL,
    rule_id TEXT NOT NULL,
    rule_version INTEGER NOT NULL CHECK (rule_version >= 1),
    subject_type TEXT NOT NULL CHECK (
        subject_type IN ('document', 'invoice', 'invoice_line', 'party', 'workspace')
    ),
    subject_id TEXT NOT NULL,
    input_json TEXT NOT NULL CHECK (json_valid(input_json)),
    output_json TEXT NOT NULL CHECK (json_valid(output_json)),
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'high')),
    occurred_at_ms INTEGER NOT NULL,
    FOREIGN KEY (rule_id, rule_version) REFERENCES rule_definitions(rule_id, version),
    UNIQUE (rule_id, rule_version, subject_type, subject_id, input_json)
) STRICT;

CREATE INDEX rule_events_occurred_index
ON rule_events(occurred_at_ms DESC, severity, rule_id);

CREATE TRIGGER rule_events_are_append_only_update
BEFORE UPDATE ON rule_events
BEGIN
    SELECT RAISE(ABORT, 'rule events are append-only');
END;

CREATE TRIGGER rule_events_are_append_only_delete
BEFORE DELETE ON rule_events
BEGIN
    SELECT RAISE(ABORT, 'rule events are append-only');
END;

CREATE TABLE rule_event_actions (
    id TEXT PRIMARY KEY NOT NULL,
    rule_event_id TEXT NOT NULL REFERENCES rule_events(id),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    action TEXT NOT NULL CHECK (action IN ('acknowledge', 'dismiss', 'reopen')),
    reason TEXT NOT NULL CHECK (length(trim(reason)) > 0),
    occurred_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX rule_event_actions_event_index
ON rule_event_actions(rule_event_id, occurred_at_ms, id);

CREATE TRIGGER rule_event_actions_are_append_only_update
BEFORE UPDATE ON rule_event_actions
BEGIN
    SELECT RAISE(ABORT, 'rule event actions are append-only');
END;

CREATE TRIGGER rule_event_actions_are_append_only_delete
BEFORE DELETE ON rule_event_actions
BEGIN
    SELECT RAISE(ABORT, 'rule event actions are append-only');
END;

CREATE TABLE dashboard_views (
    id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version >= 1),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    filters_json TEXT NOT NULL CHECK (json_valid(filters_json)),
    widget_order_json TEXT NOT NULL CHECK (json_valid(widget_order_json)),
    actor_user_id TEXT REFERENCES local_users(id),
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (id, version)
) STRICT;

INSERT INTO dashboard_views(
    id, version, name, filters_json, widget_order_json, actor_user_id, created_at_ms
) VALUES (
    '00000000-0000-0000-0000-000000000001', 1, 'Default',
    '{"periodStart":null,"periodEnd":null,"partyId":null,"category":null,"status":null,"confidence":null}',
    '["overview","aging","due_calendar","supplier_spend","category_trend","price_alerts"]',
    NULL, 0
);

CREATE TRIGGER dashboard_views_are_append_only_update
BEFORE UPDATE ON dashboard_views
BEGIN
    SELECT RAISE(ABORT, 'dashboard views are append-only');
END;

CREATE TRIGGER dashboard_views_are_append_only_delete
BEFORE DELETE ON dashboard_views
BEGIN
    SELECT RAISE(ABORT, 'dashboard views are append-only');
END;
