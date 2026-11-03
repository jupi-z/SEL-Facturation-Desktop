# SEL V2 Client Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Continue SEL V2 toward a complete, maintainable Windows client by closing the unsafe-rendering and invoice-preview gaps first, then completing documents, business workflows, administration, synchronization and operations incrementally.

**Architecture:** Preserve the native Python/pywebview shell and Rust local/central services. JavaScript remains a presentation layer and calls only the local bridge; Rust owns all financial calculation, validation and persistence.

**Tech Stack:** Rust, Axum, SQLx, SQLite/PostgreSQL, Python 3.12, pywebview, vanilla HTML/CSS/JavaScript, pytest, Cargo CI on Windows.

**Spec:** `Docs/superpowers/specs/2026-11-03-sel-v2-client-hardening-design.md`, `Docs/SEL_V2_BACKEND/`, `AGENTS.md`

## Global Constraints

- Do not merge into `main` or delete useful work.
- Preserve the `794679b` candidate and all SQLite identifiers, snapshots, history and pending synchronization operations.
- Rust is the only financial authority; no financial calculation is added to JavaScript.
- Store structured invoice data only on the central server; never store PDFs there.
- Keep water and electricity on one invoice, including a visible 0 m³ line.
- Use additive migrations only and keep the portable JSON v1 export compatible.
- No production deployment, release or real SEL database migration is performed from this worktree.

## Review Focus

- Hostile customer/payment/product/status text must render as text, not markup — covered by the safe-DOM client tests.
- Preview must not create an invoice, receivable, event or outbox row — covered by the local HTTP preview persistence test.
- A preview must contain both services and 0 m³ when requested — covered by the preview contract test.
- Confirmation must be the only path to `issue` — covered by the client interaction/static contract test.
- The pywebview bridge must reject external paths and unsupported methods — covered by the existing shell tests plus new method/path cases.

### Task 1: Harden client rendering and invoice preview

**Files:**
- Modify: `v2/client/app.js`
- Modify: `v2/client/index.html`
- Modify: `v2/client/app.css`
- Modify: `v2/crates/sel-local/src/lib.rs`
- Test: `v2/crates/sel-local/tests/local_http.rs`
- Test: `tests/test_desktop_v2.py`

**Interfaces:**
- Consumes: existing `UtilityBillingRequest`, `UtilityBillingResponse`, local tariff lookup and `POST /local/v1/billing/utility/issue`.
- Produces: `POST /local/v1/billing/utility/preview`, a non-persistent `LOCAL_PREVIEW` response, and an explicit client confirmation transition.

- [ ] Write a failing Rust test proving preview returns both service lines and leaves invoice/receivable/outbox counts unchanged.
- [ ] Run the focused Rust test and observe the missing-route failure.
- [ ] Add the preview route and shared calculation helper without persistence.
- [ ] Run the focused Rust test and verify it passes.
- [ ] Write a failing client/static test proving hostile values are not interpolated and issue is not called on preview submit.
- [ ] Run the focused Python test and observe the current unsafe interpolation/direct issue behavior.
- [ ] Replace dynamic HTML interpolation with safe DOM helpers and add preview/confirmation handlers.
- [ ] Run the focused Python tests and the full available Python suite.
- [ ] Commit `fix(v2): secure client rendering and require invoice confirmation`.

### Task 2: Client-side documents and print handoff

**Files:**
- Modify: `v2/client/app.js`, `v2/client/index.html`, `v2/client/app.css`
- Modify: `desktop_v2.py`
- Modify: `v2/crates/sel-local/src/lib.rs` only if document-data routes are missing locally
- Test: `tests/test_desktop_v2.py`, new client contract tests, targeted Rust tests

**Outcome:** Build invoice, solar-sale and payment-receipt document data from immutable JSON and hand PDFs to the local Windows shell for preview/save/print. No central PDF persistence.

### Task 3: Complete read/write business workflows

**Files:**
- Modify: `v2/client/*`
- Modify: `v2/crates/sel-local/src/lib.rs`, `sel-storage/src/sqlite.rs`, `sel-contracts/src/lib.rs` only for genuinely missing routes
- Test: local HTTP and storage integration tests

**Outcome:** Search/edit customers, select receivables from API, payment allocations, solar catalog/quotes/contracts/schedules and explicit status/error states.

### Task 4: Administration and synchronization UX

**Files:**
- Modify: `v2/client/*`, `desktop_v2.py`, Rust sync/local modules, OpenAPI and migrations only when required
- Test: auth, sync worker, two-device convergence and conflict tests

**Outcome:** First-use enrollment, durable identity association, pending/conflict visibility, manual/automatic sync, and controlled authorization states.

### Task 5: Operations, updates and evidence

**Files:**
- Modify: `Docs/SEL_V2_BACKEND/IMPLEMENTATION_REPORT.md`, runbooks, CI only where required
- Test: packaging, update, migration and release-gate checks

**Outcome:** Documented status matrix, rollback/backup procedures, update channels and explicit `BLOCKED_EXTERNAL` evidence for real Windows/server/printer/key validations.

## Execution and verification

Each task follows red → green → refactor. Before any completion claim, run the targeted test and then the full available suite. Rust/PostgreSQL/Windows-only gates remain reported as `NOT_TESTED` or `BLOCKED_EXTERNAL` unless freshly executed.
