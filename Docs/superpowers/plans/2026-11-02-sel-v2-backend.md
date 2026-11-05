# SEL V2 Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build and verify the Rust API-first SEL V2 backend under `v2/` while preserving the existing V1 application and data.

**Architecture:** A modular Rust workspace shares domain and contracts between a central Axum service using PostgreSQL and a loopback local agent using SQLite. Financial commands are decimal, idempotent, snapshot-based, and append-only; offline commands use SQLite journal/outbox plus PostgreSQL inbox/change cursor reconciliation.

**Tech Stack:** Rust stable, Tokio, Axum, SQLx, PostgreSQL, SQLite, `rust_decimal`, Serde, OpenAPI 3.1 YAML, Ed25519/SHA-256 verification, Docker Compose for test infrastructure.

**Spec:** `CODEX_SEL_V2_BACKEND_START_HERE.md`, `Docs/SEL_V2_BACKEND/`, issue #56.

## Global Constraints

- All implementation is under `v2/`; do not replace or modify the V1 Python/Flask/SQLite runtime.
- PostgreSQL stores structured business data only; the server has no PDF route, PDF bytes, invoice file path, invoice file URL, invoice attachment, or invoice binary.
- New utility invoices contain exactly electricity in kWh and water in m³; zero water is explicit and visible, while positive water requires an approved configured tariff and tax profile.
- Ordinary partial payments never create installment plans; solar installment plans reference one existing receivable and schedule amounts must equal its principal.
- `Idempotency-Key` is required on financial writes and sync commands; retries are deduplicated by stable event IDs and payload digests.
- No floats for financial data, no invented SEL rates/taxes, no DGI certification claims, and no production keys or secrets in Git.
- No merge to `main`, release, real database migration, or real deployment.

## Review Focus

- Zero-water utility issue must emit two service lines and never invent a water price; tests in `sel-domain` and `sel-api`.
- Duplicate financial writes and push retries must produce one business effect; tests in `sel-sync` and HTTP integration.
- Solar schedule totals must equal the one receivable and payments must reduce it once; tests in `sel-domain`.
- Issued snapshots and legacy imports must remain unchanged; tests in storage/CLI migration.
- No server route, schema, filesystem, log, cache, or backup may contain invoice files; schema and route scan plus runtime test.

---

### Task 1: Workspace and domain foundation

**Files:**
- Create: `v2/Cargo.toml`, `v2/crates/sel-domain/**`, `v2/crates/sel-contracts/**`
- Test: `v2/crates/sel-domain/tests/**`, `v2/crates/sel-contracts/tests/**`

**Interfaces:**
- Produces decimal `Money`, `Quantity`, tariff/tax snapshots, utility preview/issue commands, solar sale/schedule commands, payment allocation and receivable projections, problem codes, and sync envelope DTOs.

- [ ] Define UUID, decimal string, currency, service, status, role, and problem-detail types with strict Serde validation.
- [ ] Implement utility calculation with exact two-line output and zero-water confirmation; reject positive water without approved test configuration.
- [ ] Implement receivable/payment allocation and solar schedule sum invariants.
- [ ] Add FIN-01 through FIN-12 unit tests with synthetic fixtures only.
- [ ] Run `cargo test -p sel-domain -p sel-contracts` and record exact results.

### Task 2: PostgreSQL and SQLite migrations

**Files:**
- Create: `v2/crates/sel-storage/**`, `v2/migrations/postgres/*.sql`, `v2/migrations/sqlite/*.sql`
- Create: `v2/tests/schema_scan.rs`

**Interfaces:**
- Produces SQLx migration sets and repository transaction boundaries for structured records, local journal/outbox/inbox, idempotency keys, changes, conflicts, and cursors.

- [ ] Add additive PG schema for organizations, users, devices/grants, customers/services/meters, catalog snapshots, sales/invoices/lines, receivables/schedules/payments/allocations/reversals, audit, idempotency, sync, and template metadata.
- [ ] Add SQLite schema with equivalent local projections plus WAL-friendly outbox/journal/cursor tables.
- [ ] Add constraints/indexes for organization scope, unique customer/period, one receivable per obligation, schedule sum validation hooks, non-negative decimals, and unique event/idempotency keys.
- [ ] Add FILE-02 schema scan asserting no invoice file/BLOB/path/URL columns or invoice attachment relations.
- [ ] Run migration creation/repetition tests on SQLite and PostgreSQL when `TEST_DATABASE_URL` exists.

### Task 3: Central service and OpenAPI

**Files:**
- Create: `v2/crates/sel-api/**`, `v2/openapi/sel-v2.yaml`, `v2/tests/api_contract.rs`

**Interfaces:**
- Produces Axum routes for health, auth stubs with RBAC boundary, customers, catalog reads, utility preview/issue, solar sales/contracts, receivables/statements, payments, document-data, sync, and LiveUpdate metadata.

- [ ] Implement request ID, problem+json errors, organization scoping, role checks, If-Match/version checks, and Idempotency-Key enforcement.
- [ ] Implement utility preview/issue and return JSON document-data only; do not expose PDF routes.
- [ ] Implement customer/catalog read/write paths and invoice/payment/receivable projections through the domain service.
- [ ] Write OpenAPI 3.1 for every P0 route and validate it with a parser/test.
- [ ] Add API-01 through API-06 and FILE-01 contract tests.

### Task 4: Offline local agent and synchronization

**Files:**
- Create: `v2/crates/sel-sync/**`, `v2/crates/sel-local/**`, `v2/tests/sync_scenarios.rs`

**Interfaces:**
- Produces loopback local HTTP routes and sync engine APIs for atomic local command+journal+outbox, central push/pull, acknowledgements, conflicts, grants, and cursors.

- [ ] Persist local command and outbox in one SQLite transaction; use stable event IDs and payload SHA-256.
- [ ] Implement central push inbox deduplication and exact original retry result; mismatched payload becomes a conflict.
- [ ] Implement paginated pull and atomic cursor application; preserve pending outbox across rebootstrap.
- [ ] Implement safe conflict cases for duplicate period, concurrent payments, stale schedules, expired/revoked grants, and offline stock uncertainty.
- [ ] Add SYN-01 through SYN-12 with restart/interruption simulations and explicit status assertions.

### Task 5: Signed LiveUpdate and migration CLI

**Files:**
- Create: `v2/crates/sel-cli/**`, `v2/crates/sel-storage/src/migration/**`, `v2/crates/sel-sync/src/live_update.rs`, `v2/tests/live_update_and_migration.rs`

**Interfaces:**
- Produces dry-run legacy import/reconciliation, signed declarative resource verification, atomic activation/rollback metadata, and CLI commands without invoice-file handling.

- [ ] Implement manifest schema, Ed25519 detached signature, SHA-256/size/expiry/sequence/key pinning, allowed bundle types, and path traversal rejection.
- [ ] Ensure only declarative configuration/template resources can be activated; native changes require SafeUpdate and unknown code is never executed.
- [ ] Implement legacy SQLite/export v1 dry-run ID mapping and idempotent batch records; no source mutation.
- [ ] Add UPD-01 through UPD-05, MIG-01 through MIG-08, and FILE-03/FILE-04 tests where the environment permits.

### Task 6: Reproducible test deployment and evidence

**Files:**
- Create: `v2/deploy/compose.test.yml`, `v2/deploy/.env.example`, `v2/tests/e2e_cli.rs`, `Docs/SEL_V2_BACKEND/IMPLEMENTATION_REPORT.md`
- Modify: `.github/workflows/**` only if needed for Rust checks

**Interfaces:**
- Produces a local test stack, repeatable CLI scenario, CI checks, evidence files, and the final gate report.

- [ ] Add PostgreSQL + API test compose with private DB networking, pinned image, health checks, and no invoice-file volume.
- [ ] Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
- [ ] Run API/PG/SQLite/sync/live-update/migration scenarios and capture responses/commands with synthetic data.
- [ ] Enumerate all API/FIN/SYN/UPD/MIG/FILE/OPS identifiers with PASS/FAIL/BLOCKED_EXTERNAL/NOT_TESTED and exact SHA.
- [ ] Create a non-merged PR from the dedicated branch; do not release or deploy.

## Execution order

Tasks 1 and 2 establish the domain and storage contracts. Task 3 can start after contract types exist. Task 4 depends on storage and contracts. Task 5 depends on the event/storage model. Task 6 is final and may mark PostgreSQL, Windows, VPS, production keys, and real SEL validation as external blockers.

