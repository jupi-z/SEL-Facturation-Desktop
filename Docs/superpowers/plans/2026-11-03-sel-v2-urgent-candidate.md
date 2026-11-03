# SEL V2 urgent candidate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the verified SEL V2 backend into a traceable Windows-test candidate with a usable local operator shell and a documented, fail-closed server package, without merging `main` or touching real SEL data.

**Architecture:** Preserve the Rust domain/storage/API as the financial authority. Add only read-side/local-client surfaces needed by the operator shell, embed all client assets locally, and use a small Python/pywebview launcher as the Windows window and process supervisor. Package the central API separately for a controlled server deployment; no PDF or invoice file is stored centrally.

**Tech Stack:** Rust 2021, Axum, SQLx/PostgreSQL/SQLite, vanilla local HTML/CSS/JS, Python/pywebview, PyInstaller, Docker Compose, GitHub Actions.

**Spec:** `CODEX_SEL_V2_BACKEND_START_HERE.md`, `Docs/SEL_V2_BACKEND/`, Figma file `EuIpbci6IR7m6ggApo5WWm`, and the urgent delivery directive from 2026-11-03.

## Global Constraints

- Do not merge into `main`, delete branches, publish a release, or write real SEL financial data.
- Preserve V1 Python/Flask/SQLite and all historical snapshots; changes are additive.
- Rust remains the only financial calculation authority.
- PostgreSQL stores structured business data only; never store invoice PDFs, images, archives, or generated invoice files.
- No production secret, test key, or real database URL may enter Git, logs, or artifacts.
- Keep LiveUpdate/SafeUpdate fail-closed; unsigned Windows artifacts are test candidates only.
- Mark Windows, printer, TLS, production keys, and real-server checks as unverified until actually run.

## Review Focus

- Local first launch without pre-existing UUIDs: stable identities must be created and persisted by the shell, never regenerated silently.
- Offline invoice with exactly 0 m³: both utility lines must remain visible and the water confirmation must be explicit.
- Retry after a lost acknowledgement: the operator must see pending state and never a second financial effect.
- A central API failure or unavailable PostgreSQL: the Windows shell must remain usable offline and clearly expose pending synchronization.
- Packaging without WebView2 or Rust runtime files: the candidate must fail with an actionable message, not a blank window.

### Task 1: Freeze the verified baseline and report

**Files:**
- Modify: `Docs/SEL_V2_BACKEND/IMPLEMENTATION_REPORT.md`
- Create: `docs/superpowers/sdd/2026-11-03-sel-v2-urgent-candidate/progress.md`
- Test: Git state and GitHub run `41665371957`

- [ ] Record `caf0025`, green CI `41665371957`, PR #58 state, and exact remaining blockers.
- [ ] Keep all unverified production/Windows claims explicitly blocked or not tested.
- [ ] Commit the report update and verify the report references the current commit/run.

### Task 2: Complete local read-side surfaces for the operator shell

**Files:**
- Modify: `v2/crates/sel-contracts/src/lib.rs`
- Modify: `v2/crates/sel-storage/src/sqlite.rs`
- Modify: `v2/crates/sel-local/src/lib.rs`
- Test: `v2/crates/sel-local/tests/local_commercial.rs`

**Interfaces:**
- Produces local HTTP JSON reads for invoices, payments, and solar sales while reusing existing immutable summaries and organization scoping.

- [ ] Write HTTP tests for empty and populated local history reads.
- [ ] Run the focused tests and observe the expected RED state.
- [ ] Implement bounded, deterministic read methods and routes without changing financial writes.
- [ ] Run focused tests, then the full Rust workspace test suite.
- [ ] Commit the additive read surface.

### Task 3: Add a local, native-window V2 operator shell

**Files:**
- Create: `v2/client/index.html`
- Create: `v2/client/app.css`
- Create: `v2/client/app.js`
- Create: `desktop_v2.py`
- Test: `v2/crates/sel-local/tests/local_ui.rs`, Python smoke tests

**Interfaces:**
- The shell starts the bundled `sel-local` process with a persisted per-user SQLite path and stable identities, proxies UI requests with the local bearer token, and opens one pywebview window.
- The UI uses only local assets and calls local JSON APIs; it never performs financial calculations.

- [ ] Add a failing smoke test for embedded assets and the first-launch configuration contract.
- [ ] Implement the minimal dashboard, customers, combined utility invoice, statement, payment, solar, and synchronization views using the Figma journeys as layout reference.
- [ ] Implement visible offline/pending/conflict/error states and a clear 0 m³ confirmation.
- [ ] Add a guarded single-instance/process shutdown path.
- [ ] Run browserless asset/API smoke tests and Python tests.
- [ ] Commit the operator shell.

### Task 4: Prepare reproducible Windows candidate packaging

**Files:**
- Modify: `.github/workflows/windows.yml` or create `.github/workflows/sel-v2-windows.yml`
- Create: `v2/packaging/README.md`
- Create: `v2/packaging/build-v2.ps1`
- Test: GitHub Windows workflow and artifact hashes

- [ ] Compile `sel-local` and `sel-api` for Windows x64 in CI.
- [ ] Bundle the local assets, launcher, and Rust binaries without remote runtime dependencies except the explicitly documented WebView2 bootstrapper.
- [ ] Produce an unsigned acceptance artifact only; do not publish a release.
- [ ] Verify hashes and the packaging smoke test in CI.

### Task 5: Prepare a fail-closed central deployment package

**Files:**
- Create: `v2/deploy/Dockerfile`
- Create: `v2/deploy/compose.production.example.yml`
- Create: `v2/deploy/Caddyfile.example`
- Create: `v2/deploy/PRODUCTION_RUNBOOK.md`
- Test: Docker build/config validation and secret/config checks

- [ ] Build the API image from source without embedding secrets.
- [ ] Keep PostgreSQL private and expose only the API through TLS termination.
- [ ] Add health checks, migration/backup/rollback commands, and least-privilege configuration guidance.
- [ ] Add a test that rejects the production example when required secrets/identities are absent.
- [ ] Do not deploy or run against a real SEL server.

## Completion Gate

Do not label the result `GO_PRODUCTION` unless Windows acceptance, production PostgreSQL, TLS, backups/restoration, keys, financial parameters, and rollback are all directly verified. The expected session outcome is `GO_CANDIDATE` or `NO_GO`, with exact evidence.
