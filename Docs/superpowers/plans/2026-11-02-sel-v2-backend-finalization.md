# SEL V2 Backend Finalization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform the existing Rust V2 prototype into a verifiable transaction-safe backend milestone covering payment invariants, durable local operations, complete sync commands, stable identities, and migration/backup gates without changing the V1 stack or adding a frontend.

**Architecture:** Keep the existing `sel-domain` calculation rules, `sel-contracts` wire models, `sel-storage` PostgreSQL/SQLite repositories, `sel-api` central HTTP routes, and `sel-local` local HTTP agent. Add shared command application paths so local operations are journaled and projected atomically, then submit the same versioned envelopes to PostgreSQL where the central effect is committed before acknowledgement.

**Tech Stack:** Rust 2021, Axum 0.7, SQLx 0.8, PostgreSQL 16, SQLite WAL, `rust_decimal`, Tokio, serde, existing OpenAPI.

**Spec:** `CODEX_SEL_V2_BACKEND_START_HERE.md`, `Docs/SEL_V2_BACKEND/`, and the user mission for PR #58.

## Global Constraints

- Do not modify `main`, merge PR #58, publish releases, deploy to a real server, or migrate real SEL data.
- Do not build a new frontend; all verification uses HTTP/API and local tests.
- Preserve V1 data and the `sel-facturation-portable` v1 format; migrations are additive and auditable.
- PostgreSQL stores structured business data only; never store invoice PDF/image/file bytes.
- All financial calculations use `rust_decimal`; cross-currency arithmetic requires an approved snapshot.
- Ordinary partial payments do not create installment plans; one receivable remains the single financial obligation.
- Every utility invoice contains electricity and water lines, including water quantity exactly `0 m³`.
- Production mode must fail closed when required security and identity configuration is missing.

## Review Focus

- Duplicate payment allocations to the same receivable must be aggregated before validation and rejected atomically if the aggregate exceeds the balance.
- A retry after PostgreSQL commit but before the client acknowledgement must replay the durable idempotent result without creating another payment.
- A local command must survive process restart with its journal and outbox state intact and must not be marked accepted before central commit.
- Two devices issuing the same aggregate or period concurrently must converge to one accepted financial obligation and an auditable conflict/rejection.
- A missing or changing organization/device identity must fail startup rather than silently creating a new identity.

### Task 1: Payment allocation aggregate and concurrency invariants

**Files:**
- Modify: `v2/crates/sel-storage/src/postgres.rs`
- Modify: `v2/crates/sel-storage/tests/postgres_commercial.rs`
- Create: `v2/crates/sel-storage/tests/postgres_payments_concurrency.rs`

**Interfaces:**
- Consumes: `RecordPaymentRequest`, `PaymentAllocationRequest`, existing `CentralStore::record_payment`.
- Produces: deterministic rejection for duplicate receivable entries whose aggregate exceeds the locked remaining balance; idempotent replay for accepted retries.

- [ ] Write failing tests for two allocations of 7 against a 10 receivable, concurrent payments, and commit-before-ack retry.
- [ ] Run the focused PostgreSQL tests and observe the pre-fix failure.
- [ ] Group allocations by receivable, reject non-positive/invalid values, lock each distinct receivable in deterministic UUID order, then validate the aggregate against one balance query inside the transaction.
- [ ] Insert the payment and allocations only after all groups pass; preserve idempotency and commit semantics.
- [ ] Run focused and full PostgreSQL tests.
- [ ] Commit `fix(v2): make payment allocation validation aggregate and concurrent-safe`.

### Task 2: Stable identities and fail-closed production security configuration

**Files:**
- Modify: `v2/crates/sel-api/src/main.rs`
- Modify: `v2/crates/sel-local/src/main.rs`
- Modify: `v2/crates/sel-storage/src/postgres.rs`
- Modify: `v2/deploy/.env.example`
- Create: `v2/crates/sel-api/tests/security_config.rs`

**Interfaces:**
- Consumes: configured organization/device UUIDs, token/session configuration, existing `with_production_central_store`.
- Produces: startup configuration validation and stable identity loading with explicit first-enrollment behavior.

- [ ] Write failing tests for absent/invalid organization and device identity and absent production auth configuration.
- [ ] Run them to verify the expected fail-closed failures.
- [ ] Add typed configuration parsing; require persisted organization identity for central mode and persisted device identity for local mode; do not generate identities in production.
- [ ] Keep development/test constructors explicit and unavailable to production startup.
- [ ] Run security and workspace tests.
- [ ] Commit `fix(v2): fail closed on missing production identities and auth`.

### Task 3: Durable local commercial command application

**Files:**
- Modify: `v2/crates/sel-local/src/lib.rs`
- Modify: `v2/crates/sel-storage/src/sqlite.rs`
- Modify: `v2/migrations/sqlite/0001_init.sql`
- Modify: `v2/crates/sel-local/tests/local_http.rs`
- Create: `v2/crates/sel-local/tests/local_commercial.rs`

**Interfaces:**
- Consumes: existing `LocalStore`, `CreateCustomer`, `UtilityBillingRequest`, payment/solar contracts.
- Produces: local HTTP endpoints for customers, document data, receivables, payments, and solar operations; each write atomically journals command, local projection, and outbox.

- [ ] Write failing HTTP tests for local customer creation/read, combined utility invoice with 0 m³, partial payment, and durable restart state.
- [ ] Run them and verify missing local routes/behavior.
- [ ] Add SQLite projection tables/queries and shared validation through `sel-domain`; retain command journal/outbox in the same transaction.
- [ ] Expose only structured JSON; mark unacknowledged results `LOCAL_PENDING`.
- [ ] Run local durability and workspace tests.
- [ ] Commit `feat(v2): expose durable offline commercial operations`.

### Task 4: Complete versioned sync command application and worker

**Files:**
- Modify: `v2/crates/sel-contracts/src/lib.rs`
- Modify: `v2/crates/sel-storage/src/postgres.rs`
- Modify: `v2/crates/sel-storage/src/sqlite.rs`
- Modify: `v2/crates/sel-sync/src/lib.rs`
- Modify: `v2/crates/sel-local/src/lib.rs`
- Create: `v2/crates/sel-sync/tests/sync_recovery.rs`
- Modify: `v2/crates/sel-api/tests/postgres_sync.rs`

**Interfaces:**
- Consumes: `SyncEnvelope`, durable outbox/journal, `CentralStore::apply_sync_event`.
- Produces: command handlers for customer updates, utility invoices, solar sale/contract, payments, and corrections with persisted ACK/conflict/rejection; retryable worker state.

- [ ] Add failing tests for each command type, duplicate replay, conflict, central-commit-before-ack, and restart recovery.
- [ ] Run them to verify unsupported-command failures.
- [ ] Normalize command names and implement central application using existing transactional methods, preserving one durable sync inbox record per event.
- [ ] Implement local outbox claiming, retry/backoff state, ACK application and central change pull transactionally.
- [ ] Run two-device and interruption simulations against disposable PostgreSQL/SQLite.
- [ ] Commit `feat(v2): complete durable offline synchronization commands`.

### Task 5: Financial corrections, schedules, and audit-only reversals

**Files:**
- Modify: `v2/crates/sel-domain/src/billing.rs`
- Modify: `v2/crates/sel-domain/src/solar.rs`
- Modify: `v2/crates/sel-storage/src/postgres.rs`
- Modify: `v2/migrations/postgres/0001_init.sql`
- Modify: `v2/crates/sel-api/tests/postgres_commercial.rs`
- Modify: `v2/crates/sel-api/tests/postgres_solar.rs`

**Interfaces:**
- Consumes: existing receivable/payment/schedule tables and decimal domain functions.
- Produces: append-only reversal/correction operations, installment payment allocation, overdue summaries, and exact currency/rounding checks.

- [ ] Write failing tests for reversal, void/correction, schedule allocation, overdue status, currency mismatch, and ordinary partial payment without a plan.
- [ ] Run focused tests to verify missing behavior.
- [ ] Add append-only financial events and transactional derived-state updates without deleting historical facts.
- [ ] Ensure schedule rows allocate to the same receivable and cannot create a second principal.
- [ ] Run full finance regression tests.
- [ ] Commit `feat(v2): complete append-only receivable corrections and schedules`.

### Task 6: V1 import, PostgreSQL backup/restore tooling, documentation and CI gates

**Files:**
- Modify: `v2/crates/sel-cli/src/main.rs`
- Create: `v2/crates/sel-cli/tests/migration_import.rs`
- Create: `v2/scripts/backup-postgres.ps1`
- Create: `v2/scripts/restore-postgres.ps1`
- Modify: `.github/workflows/sel-v2-backend.yml`
- Modify: `Docs/SEL_V2_BACKEND/IMPLEMENTATION_REPORT.md`

**Interfaces:**
- Consumes: portable v1 JSON, CentralStore repositories, disposable PostgreSQL service.
- Produces: simulation-then-approval import with idempotency/reconciliation, reproducible backup/restore scripts, CI coverage and final evidence.

- [ ] Write failing migration tests for duplicate-safe import, totals reconciliation, backup-before-write, and failure recovery.
- [ ] Run them to verify the current dry-run-only limitation.
- [ ] Implement transactional import with explicit `--apply` and required backup path; keep default simulation read-only.
- [ ] Add CI jobs for concurrency, local durability, sync recovery, and migration checks.
- [ ] Run all gates, update the report with exact outputs and limitations, and commit `test(v2): add final backend validation gates`.

## Final Verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] PostgreSQL integration and concurrency tests against disposable PostgreSQL 16
- [ ] SQLite durability and offline/restart tests
- [ ] Sync two-device and commit-before-ACK recovery tests
- [ ] Migration simulation/import tests with synthetic portable data only
- [ ] `git diff --check`
- [ ] GitHub Actions green on the final commit
- [ ] PR #58 updated; no merge, release, deployment, or real-data migration
