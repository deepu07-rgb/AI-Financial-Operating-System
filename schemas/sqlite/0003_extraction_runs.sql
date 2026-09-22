CREATE TABLE extraction_runs (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(id),
    protocol_version INTEGER NOT NULL CHECK (protocol_version >= 1),
    pipeline_version TEXT NOT NULL CHECK (length(trim(pipeline_version)) > 0),
    engine_name TEXT NOT NULL CHECK (length(trim(engine_name)) > 0),
    engine_version TEXT NOT NULL CHECK (length(trim(engine_version)) > 0),
    language TEXT NOT NULL CHECK (length(trim(language)) > 0),
    result_json TEXT NOT NULL CHECK (json_valid(result_json)),
    requires_review INTEGER NOT NULL CHECK (requires_review IN (0, 1)),
    started_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER NOT NULL CHECK (completed_at_ms >= started_at_ms),
    duration_ms INTEGER NOT NULL CHECK (duration_ms >= 0)
) STRICT;

CREATE INDEX extraction_runs_document_index
ON extraction_runs(document_id, completed_at_ms, id);

CREATE TRIGGER extraction_runs_are_append_only_update
BEFORE UPDATE ON extraction_runs
BEGIN
    SELECT RAISE(ABORT, 'extraction runs are append-only');
END;

CREATE TRIGGER extraction_runs_are_append_only_delete
BEFORE DELETE ON extraction_runs
BEGIN
    SELECT RAISE(ABORT, 'extraction runs are append-only');
END;
