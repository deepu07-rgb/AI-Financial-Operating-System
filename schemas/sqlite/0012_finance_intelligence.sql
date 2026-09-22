CREATE TABLE supplier_finance_profiles (
    party_id TEXT NOT NULL REFERENCES parties(id),
    version INTEGER NOT NULL CHECK (version >= 1),
    criticality INTEGER CHECK (criticality IS NULL OR criticality BETWEEN 1 AND 5),
    penalty_basis_points INTEGER CHECK (
        penalty_basis_points IS NULL OR penalty_basis_points BETWEEN 0 AND 100000
    ),
    terms_evidence TEXT CHECK (
        terms_evidence IS NULL OR length(trim(terms_evidence)) BETWEEN 1 AND 500
    ),
    actor_user_id TEXT NOT NULL REFERENCES local_users(id),
    reason TEXT NOT NULL CHECK (length(trim(reason)) BETWEEN 1 AND 500),
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY (party_id, version)
) STRICT;

CREATE TRIGGER supplier_finance_profiles_are_append_only_update
BEFORE UPDATE ON supplier_finance_profiles
BEGIN
    SELECT RAISE(ABORT, 'supplier finance profiles are append-only');
END;

CREATE TRIGGER supplier_finance_profiles_are_append_only_delete
BEFORE DELETE ON supplier_finance_profiles
BEGIN
    SELECT RAISE(ABORT, 'supplier finance profiles are append-only');
END;

CREATE INDEX supplier_finance_profiles_latest
ON supplier_finance_profiles(party_id, version DESC);

-- CR-04 role-relevant defaults use the operational finance filter contract. They
-- remain append-only versions so an existing workspace upgrades without losing
-- the historical WP-06 definition.
INSERT INTO dashboard_views(
    id, version, name, filters_json, widget_order_json, actor_user_id, created_at_ms
) VALUES (
    '00000000-0000-0000-0000-000000000001', 2, 'Owner / CFO daily decisions',
    '{"periodStart":null,"periodEnd":null,"partyId":null,"categoryId":null,"direction":null,"obligationState":null,"currency":null}',
    '["top_actions","exposure","cash_pressure","exceptions","coverage"]',
    NULL, 0
);

INSERT INTO dashboard_views(
    id, version, name, filters_json, widget_order_json, actor_user_id, created_at_ms
) VALUES (
    '00000000-0000-0000-0000-000000000002', 1, 'Payables operations',
    '{"periodStart":null,"periodEnd":null,"partyId":null,"categoryId":null,"direction":"payable","obligationState":null,"currency":null}',
    '["top_actions","aging","payment_priority","exceptions"]',
    NULL, 0
);
