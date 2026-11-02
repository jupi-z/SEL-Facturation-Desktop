# SEL V2 Critical Backend Finalization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** close the P0 reconciliation and durability gaps, then prove bidirectional convergence between two SQLite clients and PostgreSQL without duplicate financial effects.

**Architecture:** Preserve the existing Rust workspace and SQLx repositories. Use stable client identifiers, transactionally project central responses into SQLite, and treat central sync inbox/change publication as one PostgreSQL transaction. Extend only the local commercial/sync boundaries needed by the tests; do not start a frontend or store PDFs server-side.

**Tech Stack:** Rust 1.85, Axum, SQLx, PostgreSQL, SQLite WAL, Tokio, rust_decimal, serde, reqwest, Ed25519/SHA-256 update verifier.

**Spec:** `CODEX_SEL_V2_BACKEND_START_HERE.md`, `Docs/SEL_V2_BACKEND/`, and the current mission message.

## Global Constraints

- No frontend work, no merge into `main`, no release, no real-data migration and no deployment.
- PostgreSQL stores structured business data only; it must never store PDF/image invoice files.
- Utility invoices always contain electricity in kWh and water in m³, including a visible zero-water line.
- Stable UUIDs are generated before synchronization and must be preserved across retries and projections.
- Financial writes are Decimal-based, transactional, idempotent and append-only for corrections/reversals.
- No tariff, tax, interest, fiscal reference or production key may be invented.
- Production authentication must fail closed; test-only permissive constructors remain explicitly test-scoped.

## Review Focus

- A central acceptance must update a local pending invoice/payment without treating a status-only difference as a financial conflict.
- Replaying an accepted event after a lost acknowledgement must not create a second invoice, receivable, payment or allocation.
- A committed financial effect must always have a durable sync acknowledgement and central change publication.
- Two SQLite stores must converge to the same stable IDs, amounts, snapshots, balances and statuses after pull/replay.
- A failure before or after central commit must leave a retryable, auditable state rather than silently dropping an operation.

---

### Task 1: Reproduce and fix local invoice/payment reconciliation

**Files:**
- Modify: `v2/crates/sel-storage/src/sqlite.rs`
- Modify: `v2/crates/sel-local/src/lib.rs`
- Test: `v2/crates/sel-local/tests/sync_worker.rs`
- Test: `v2/crates/sel-local/tests/local_commercial.rs`

**Interfaces:**
- Consumes `SyncPullResponse`, `UtilityBillingResponse`, `PaymentSyncPayload` and existing `apply_pulled_changes`.
- Produces idempotent local projection semantics: same stable entity and same financial snapshot may update lifecycle status; a different immutable snapshot becomes a durable conflict.

- [ ] Write a failing invoice test where a local `LOCAL_PENDING` invoice receives a central `CENTRAL_ACCEPTED` projection with identical IDs/lines/snapshot and assert it is updated, not rejected or duplicated.
- [ ] Run the focused test and observe the expected failure against the current projection.
- [ ] Write a failing payment test where a local pending payment receives the accepted central payment and assert one payment, one allocation set and `CENTRAL_ACCEPTED` status.
- [ ] Run the focused test and observe the expected failure.
- [ ] Implement minimal status-aware projection with immutable financial snapshot comparison and conflict recording only for true financial divergence.
- [ ] Run both focused tests and the local sync suite; require all pass.
- [ ] Commit the reconciliation fix.

### Task 2: Prove central atomic publication and retry semantics

**Files:**
- Modify: `v2/crates/sel-storage/src/postgres.rs`
- Modify: `v2/crates/sel-api/src/lib.rs` only if repository wiring requires it
- Test: `v2/crates/sel-storage/tests/postgres_sync_financial.rs`
- Test: `v2/crates/sel-storage/tests/postgres_persistence.rs`

**Interfaces:**
- Consumes `apply_sync_event`, `persist_sync_result`, `central_changes`, `sync_inbox`, financial repositories and idempotency tables.
- Produces a transactionally durable result: financial effect, sync inbox result, and central change publication commit together; replay returns `DUPLICATE` without a second effect.

- [ ] Add a failing PostgreSQL test that checks accepted financial effects, `sync_inbox` and `central_changes` in one committed transaction, then replays the exact event and asserts unchanged counts.
- [ ] Add a failure-injection test at the repository boundary if the existing transaction seam permits it; otherwise document the unavailable injection point and test the durable retry boundary using a dropped acknowledgement.
- [ ] Run focused PostgreSQL tests against the real test PostgreSQL and record the failure before implementation.
- [ ] Correct only the transaction/outbox publication boundary; preserve existing idempotency keys and stable IDs.
- [ ] Re-run focused PostgreSQL tests, then the complete storage integration suite.
- [ ] Commit the atomic publication fix.

### Task 3: Build the mandatory two-SQLite convergence scenario

**Files:**
- Create: `v2/crates/sel-local/tests/two_device_convergence.rs`
- Modify: `v2/crates/sel-local/src/lib.rs` only for testable sync seams
- Modify: `v2/crates/sel-storage/src/sqlite.rs` only for missing projection/state semantics
- Modify: `v2/crates/sel-storage/tests/support/mod.rs` if shared PostgreSQL setup is needed

**Interfaces:**
- Consumes two independent `LocalStore` instances, real `CentralStore`, `sync_once`, `pull_once`, SQLite reopen behavior and the stable financial contracts.
- Produces an automated scenario covering offline invoice/payment, restart, reconnect, replay, interruption points and explicit concurrent payment conflicts.

- [ ] Write the reference test first: two SQLite databases plus real PostgreSQL; assert the intended stable IDs, totals, snapshots, receivable balance and payment status.
- [ ] Run it and observe the first missing-operation or projection failure.
- [ ] Implement only the missing seams required for the reference scenario, not a new frontend or duplicate business engine.
- [ ] Add replay and interruption cases at push/pull boundaries; assert no duplicate rows and auditable conflict/retry state.
- [ ] Add the solar sale + schedule variant with exactly one receivable and one installment plan.
- [ ] Run the complete two-device test repeatedly against a clean PostgreSQL test database.
- [ ] Commit the convergence evidence and implementation.

### Task 4: Complete local solar/schedule and financial correction boundaries

**Files:**
- Modify: `v2/crates/sel-contracts/src/lib.rs`
- Modify: `v2/crates/sel-storage/src/sqlite.rs`
- Modify: `v2/crates/sel-local/src/lib.rs`
- Test: `v2/crates/sel-local/tests/local_commercial.rs`
- Test: `v2/crates/sel-local/tests/sync_worker.rs`

**Interfaces:**
- Consumes existing solar DTOs/domain validation and existing SQLite tables.
- Produces local durable commands for solar sale, schedule activation and payment allocation to schedules; ordinary utility partial payments remain non-scheduled.

- [ ] Add failing tests for local solar sale, one schedule, payment against a schedule, and ordinary partial payment without auto-schedule.
- [ ] Run focused tests and observe missing route/storage behavior.
- [ ] Implement transactionally journaled SQLite operations and outbox payloads using the shared DTOs/domain validation.
- [ ] Add append-only reversal/correction contracts only where the existing schema and normative docs define them; otherwise classify as `NOT_TESTED` rather than inventing accounting rules.
- [ ] Run local and central financial tests and commit.

### Task 5: Security and operational gate audit

**Files:**
- Modify: `v2/crates/sel-api/src/lib.rs` only for demonstrably missing production guard behavior
- Modify: `v2/crates/sel-local/src/main.rs` only for fail-closed configuration
- Test: existing API/local security tests plus new focused tests if a gap is found
- Modify: `Docs/SEL_V2_BACKEND/IMPLEMENTATION_REPORT.md`

**Interfaces:**
- Consumes existing bearer guard, identity persistence, update verifier and migrations.
- Produces an explicit PASS/FAIL/NOT_TESTED/BLOCKED_EXTERNAL matrix without claiming user/RBAC/offline-grant features that are not implemented.

- [ ] Audit and test the current production startup guards, ID stability and no-PDF invariant.
- [ ] Implement only security fixes proven by a failing test; do not weaken cryptographic checks or invent production keys.
- [ ] Run fmt, Clippy, full Rust tests, real PostgreSQL tests and migration/file-storage tests.
- [ ] Update the report with exact commits, CI run, reference test results and residual gaps.
- [ ] Commit the final report update.

### Task 6: Final branch and PR verification

**Files:**
- Modify: `Docs/SEL_V2_BACKEND/BRANCH_AUDIT_2026-11-02.md` only if the remote state changed
- Modify: `Docs/SEL_V2_BACKEND/IMPLEMENTATION_REPORT.md` only for final evidence

- [ ] Verify the PR head, base, merge state and latest GitHub Actions conclusion.
- [ ] Verify `main` was not changed and no release/deployment was created.
- [ ] Verify remote branches; preserve any branch with unintegrated work and delete only explicitly obsolete branches.
- [ ] Add the final PR comment with exact evidence and separate technical/pilot/production verdicts.
- [ ] Do not merge.
