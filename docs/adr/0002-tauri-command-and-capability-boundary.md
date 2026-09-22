# ADR-0002: Tauri Command and Capability Boundary

- Status: Accepted for WP-01
- Date: 2026-08-04
- Owners: Founder and technical reviewer
- PRD references: Sections 11.1–11.2, 17.2, 18; NFR-ACC-001,
  NFR-LOC-001; WP-01

## Context

The frontend must not access arbitrary filesystem paths, local secrets, or
network destinations. WP-01 needs one small persistence feature for local
interface preferences without introducing SQLite or broad Tauri plugins before
their work packages.

## Decision

Use explicit custom Tauri commands as the frontend/native boundary. WP-01
exposes only `get_local_settings` and `save_local_settings`. The Rust process
resolves its own application configuration directory and accepts a strict,
versioned settings structure. The webview receives `core:default` permissions
only; filesystem, shell, HTTP, process, dialog, updater, and store plugins are
not installed or permitted.

Navigation remains a small typed in-application router using the browser history
API. This avoids a routing dependency while the shell has only ten static
top-level routes. Reconsider only if nested/data-aware routing becomes complex.

## Consequences

- Native access remains narrow and auditable.
- Browser preview degrades safely to session-only preferences.
- UI settings use a small JSON file until the operational database is
  introduced in WP-02.
- The current replace-write implementation assumes one running desktop process;
  multi-process concurrency requires a later lock or centralized settings store.
- Every future native capability requires dependency/license and threat-model
  review plus an explicit permission change.

## Validation

- Frontend unit/render tests cover routing, safe defaults, and honest empty
  states.
- Rust tests cover settings defaults, serialization, validation, and persistence.
- CI builds the frontend and runs Rust formatting, tests, and Clippy.
- Native Tauri launch is verified on Windows. Exact 125% display-scaling and
  keyboard checks remain part of pilot-hardware validation.
