CREATE TABLE document_review_transitions (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    document_id TEXT NOT NULL REFERENCES documents(id),
    extraction_run_id TEXT REFERENCES extraction_runs(id),
    actor_user_id TEXT REFERENCES local_users(id),
    from_status TEXT CHECK (
        from_status IS NULL OR from_status IN (
            'imported', 'processing', 'needs_review', 'approved', 'rejected', 'exported'
        )
    ),
    to_status TEXT NOT NULL CHECK (
        to_status IN (
            'imported', 'processing', 'needs_review', 'approved', 'rejected', 'exported'
        )
    ),
    reason TEXT NOT NULL CHECK (length(trim(reason)) > 0),
    occurred_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX document_review_current_index
ON document_review_transitions(document_id, sequence DESC);

CREATE TRIGGER document_review_transitions_are_append_only_update
BEFORE UPDATE ON document_review_transitions
BEGIN
    SELECT RAISE(ABORT, 'document review transitions are append-only');
END;

CREATE TRIGGER document_review_transitions_are_append_only_delete
BEFORE DELETE ON document_review_transitions
BEGIN
    SELECT RAISE(ABORT, 'document review transitions are append-only');
END;

INSERT INTO document_review_transitions(
    id, document_id, extraction_run_id, actor_user_id, from_status,
    to_status, reason, occurred_at_ms
)
SELECT id, id, NULL, NULL, NULL, 'imported', 'Existing document entered review workflow', imported_at_ms
FROM documents;

CREATE TABLE field_corrections (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    document_id TEXT NOT NULL REFERENCES documents(id),
    extraction_run_id TEXT NOT NULL REFERENCES extraction_runs(id),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    field_name TEXT NOT NULL CHECK (length(trim(field_name)) > 0),
    original_value_json TEXT NOT NULL CHECK (json_valid(original_value_json)),
    prior_value_json TEXT NOT NULL CHECK (json_valid(prior_value_json)),
    new_value_json TEXT NOT NULL CHECK (json_valid(new_value_json)),
    reason TEXT NOT NULL CHECK (length(trim(reason)) > 0),
    occurred_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX field_corrections_document_index
ON field_corrections(document_id, extraction_run_id, field_name, sequence);

CREATE TRIGGER field_corrections_are_append_only_update
BEFORE UPDATE ON field_corrections
BEGIN
    SELECT RAISE(ABORT, 'field corrections are append-only');
END;

CREATE TRIGGER field_corrections_are_append_only_delete
BEFORE DELETE ON field_corrections
BEGIN
    SELECT RAISE(ABORT, 'field corrections are append-only');
END;

CREATE TABLE review_comments (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    document_id TEXT NOT NULL REFERENCES documents(id),
    extraction_run_id TEXT REFERENCES extraction_runs(id),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    body TEXT NOT NULL CHECK (length(trim(body)) > 0),
    tags_json TEXT NOT NULL CHECK (
        json_valid(tags_json) AND json_type(tags_json) = 'array'
    ),
    occurred_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX review_comments_document_index
ON review_comments(document_id, sequence);

CREATE TRIGGER review_comments_are_append_only_update
BEFORE UPDATE ON review_comments
BEGIN
    SELECT RAISE(ABORT, 'review comments are append-only');
END;

CREATE TRIGGER review_comments_are_append_only_delete
BEFORE DELETE ON review_comments
BEGIN
    SELECT RAISE(ABORT, 'review comments are append-only');
END;
