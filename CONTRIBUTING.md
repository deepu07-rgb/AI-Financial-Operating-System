# Contributing to AIFOS

## Required workflow

1. Work on one PRD work package at a time.
2. Cite the relevant requirement or work-package ID in the issue and change.
3. Describe dependency, security, privacy, migration, and rollback impacts.
4. Use synthetic fixtures only. Never commit customer financial data, secrets,
   credentials, access tokens, or provider API keys.
5. Add or update tests and documentation with each behavior change.
6. Run `pnpm verify` and `pnpm audit --audit-level high` before review.

## Scope boundaries

Do not add a local LLM, blockchain, cryptocurrency, autonomous payments, tax
filing, or a full accounting/ERP implementation to the MVP. Financial
calculations must be deterministic, repeatable, and traceable to input records.

## Dependencies

Every direct dependency or GitHub Action must be pinned exactly and entered in
`docs/licensing/DEPENDENCIES.md` before use. Record its version or commit,
license, purpose, alternatives considered, and review date. Regenerate the
license report and SBOM after approval. GPL, AGPL, LGPL, model weights, and
unknown/custom licenses require explicit legal review before introduction.

## Architecture decisions

Use `docs/adr/ADR_TEMPLATE.md` for decisions that are expensive to reverse,
change a trust boundary, add a data store or network flow, or constrain future
providers/connectors. Proposed ADRs are not approvals.

## Pull requests

Keep changes small and reviewable. A pull request should include:

- PRD IDs and acceptance criteria covered;
- files/modules and migrations changed;
- dependency and license changes;
- security/privacy and offline behavior impact;
- tests run with results;
- open risks and rollback notes.
