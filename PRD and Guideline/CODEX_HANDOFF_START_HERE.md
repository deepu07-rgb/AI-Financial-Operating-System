# AIFOS Codex Handoff - Start Here

## Source of truth

Use `AI_Financial_Operating_System_PRD_Blueprint_v1.0.md` as the implementation source of truth. The DOCX and PDF are human-readable reference editions. Do not infer requirements from the older v0.1 blueprint.

## Mandatory product boundaries

- Build an offline-first Windows desktop intelligence layer, not a full ERP or accounting replacement.
- Start with English printed invoices and a human review step.
- Do not add a local LLM, blockchain, cryptocurrency, autonomous payments, tax filing, or bank-transfer execution in the MVP.
- Keep financial calculations deterministic, tested, and traceable to source records.
- Put cloud AI behind a provider-neutral internal gateway and require explicit user consent before sending selected data.
- Use synthetic fixtures only; never commit customer financial data, credentials, or API keys.
- Treat all third-party licenses as a release gate. Recheck the exact dependency version and selected model artifact before distribution.

## First Codex task

Copy the prompt below into Codex after placing the PRD Markdown file in the repository root.

```text
You are implementing the AI Financial Operating System described in
AI_Financial_Operating_System_PRD_Blueprint_v1.0.md.

Treat the PRD as the product source of truth. Start with WP-00 only.

Before making changes:
1. Inspect the repository and current files.
2. Summarize the intended architecture and identify conflicts with the PRD.
3. Propose a small implementation plan for WP-00.
4. List every new dependency with its exact version, license, and purpose.

Implementation constraints:
- Offline-first Windows desktop.
- React + TypeScript frontend in a Tauri shell.
- SQLite operational database; DuckDB only where analytical queries justify it.
- Python document worker behind a narrow versioned job API.
- No local LLM, blockchain, autonomous payments, or full ERP in MVP.
- Financial calculations must be deterministic and tested.
- AI provider integration must remain behind an internal adapter.
- Never commit real financial data or secrets.
- Add tests, documentation, ADRs, and verification output.

Complete only WP-00, then stop and provide:
- files changed;
- commands run and results;
- open risks;
- recommended next work package.
```

## Review gate before WP-01

Proceed only after all of the following are demonstrated:

- repository structure and contribution rules exist;
- CI runs linting, type checks, and tests;
- dependency and license reporting exists;
- security and threat-model skeletons exist;
- no product feature beyond WP-00 was prematurely implemented;
- the founder has selected the commercial license strategy and initial pilot vertical.

## Recommended working method

Open one work package at a time. Require Codex to cite the PRD requirement IDs it is implementing, show the plan before editing, run verification commands, and stop at the package boundary for review.
