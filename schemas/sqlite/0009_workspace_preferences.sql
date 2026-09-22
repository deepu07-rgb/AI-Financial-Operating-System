CREATE TABLE workspace_preferences (
    workspace_id TEXT PRIMARY KEY NOT NULL REFERENCES workspaces(id),
    financial_year_start_month INTEGER NOT NULL CHECK (
        financial_year_start_month BETWEEN 1 AND 12
    ),
    date_format TEXT NOT NULL CHECK (
        date_format IN ('DD/MM/YYYY', 'MM/DD/YYYY', 'YYYY-MM-DD')
    ),
    tax_label TEXT CHECK (
        tax_label IS NULL OR length(trim(tax_label)) BETWEEN 1 AND 40
    )
) STRICT;

INSERT INTO workspace_preferences(
    workspace_id, financial_year_start_month, date_format, tax_label
)
SELECT id, 4, 'DD/MM/YYYY', 'GST'
FROM workspaces;

