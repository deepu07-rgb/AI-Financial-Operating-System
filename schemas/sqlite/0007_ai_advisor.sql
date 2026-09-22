CREATE TABLE ai_analysis_requests (
    id TEXT PRIMARY KEY NOT NULL,
    report_run_id TEXT NOT NULL REFERENCES report_runs(id),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    task TEXT NOT NULL CHECK (task IN (
        'profit_movement', 'payment_priority', 'overdue_exposure',
        'supplier_trend', 'cash_scenario'
    )),
    mode TEXT NOT NULL CHECK (mode IN ('economy', 'standard')),
    question TEXT NOT NULL CHECK (length(trim(question)) BETWEEN 1 AND 2000),
    consent_receipt_json TEXT NOT NULL CHECK (json_valid(consent_receipt_json)),
    context_json TEXT NOT NULL CHECK (json_valid(context_json)),
    context_sha256 TEXT NOT NULL CHECK (
        length(context_sha256) = 64 AND context_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    approximate_record_count INTEGER NOT NULL CHECK (approximate_record_count BETWEEN 1 AND 250),
    created_at_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX ai_analysis_requests_created_index
ON ai_analysis_requests(created_at_ms DESC);

CREATE TRIGGER ai_analysis_requests_are_append_only_update
BEFORE UPDATE ON ai_analysis_requests
BEGIN
    SELECT RAISE(ABORT, 'AI requests are immutable until history deletion');
END;

CREATE TABLE ai_analysis_answers (
    request_id TEXT PRIMARY KEY NOT NULL REFERENCES ai_analysis_requests(id) ON DELETE CASCADE,
    response_json TEXT NOT NULL CHECK (json_valid(response_json)),
    response_sha256 TEXT NOT NULL CHECK (
        length(response_sha256) = 64 AND response_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    provider_id TEXT NOT NULL CHECK (length(trim(provider_id)) BETWEEN 1 AND 120),
    model_id TEXT NOT NULL CHECK (length(trim(model_id)) BETWEEN 1 AND 160),
    input_units INTEGER NOT NULL CHECK (input_units >= 0),
    output_units INTEGER NOT NULL CHECK (output_units >= 0),
    cost_micros INTEGER NOT NULL CHECK (cost_micros >= 0),
    completed_at_ms INTEGER NOT NULL
) STRICT;

CREATE TRIGGER ai_analysis_answers_are_append_only_update
BEFORE UPDATE ON ai_analysis_answers
BEGIN
    SELECT RAISE(ABORT, 'AI answers are append-only until history deletion');
END;
