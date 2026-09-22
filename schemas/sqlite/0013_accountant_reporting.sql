CREATE TABLE report_reminders (
    id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version >= 1),
    template_id TEXT NOT NULL,
    cadence TEXT NOT NULL CHECK(cadence IN ('manual', 'weekly', 'monthly', 'quarterly')),
    next_due_date TEXT CHECK(next_due_date IS NULL OR (length(next_due_date) = 10 AND next_due_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')),
    enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
    actor_user_id TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY(id, version),
    FOREIGN KEY(actor_user_id) REFERENCES local_users(id)
);

CREATE INDEX report_reminders_latest ON report_reminders(id, version DESC);
CREATE INDEX report_reminders_due ON report_reminders(enabled, next_due_date);

CREATE TRIGGER report_reminders_are_append_only_update
BEFORE UPDATE ON report_reminders BEGIN
    SELECT RAISE(ABORT, 'report reminders are append-only');
END;

CREATE TRIGGER report_reminders_are_append_only_delete
BEFORE DELETE ON report_reminders BEGIN
    SELECT RAISE(ABORT, 'report reminders are append-only');
END;

INSERT INTO report_templates(
    id, template_key, version, name, definition_json, implementation_status, actor_user_id, created_at_ms
) VALUES (
    '00000000-0000-0000-0000-000000000206', 'RPT-006', 2,
    'Obligations and settlements', '{}', 'active', NULL, 0
);
