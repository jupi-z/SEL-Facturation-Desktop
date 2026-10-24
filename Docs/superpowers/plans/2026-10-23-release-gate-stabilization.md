# SEL Release Gate Stabilization Plan

> **Execution:** Native implementation on the existing PR #47 branch. Do not merge or publish.

**Goal:** Resolve financial, tariff, water-connectivity, historical-data, and PDF proof gaps required by Issue #49.

**Architecture:** Keep Flask/SQLite/Decimal/ReportLab and SafeUpdate intact. Convert a copy of the immutable invoice snapshot for CDF display; make active/confirmed tariff selection deterministic; keep physical water connection distinct from mandatory invoice presentation.

**Tech Stack:** Python 3.12, Flask, SQLite, Decimal, ReportLab, pypdf, PyInstaller, WiX, GitHub Actions.

**Spec:** `Docs/SEL_RELEASE_GATE_V1/00_MISSION_CODEX.md` through `05_PLAN_CONTROLE_ET_RAPPORT.md` and `Docs/SEL_BILLING_COMBINED_V1/01_SPECIFICATION_NORMATIVE.md`.

## Global Constraints

- SQLite migrations are additive/idempotent and preserve real data, snapshots, invoices, payments, numbering, backups, and portable JSON v1.
- New invoices have exactly electricity and water lines; 0 m³ remains explicit; positive water stays blocked without SEL-confirmed tariff/tax.
- No invented business/tax data; QR remains internal; SafeUpdate remains fail-closed on Ed25519.
- No main merge, release, deployment, real-data mutation, Windows-protection bypass, or production key.

## Review Focus

- CDF HTTP PDF can diverge from its source snapshot or mix units; verify actual generated documents with pypdf and arithmetic reconciliation.
- An inactive newest tariff must not silently select an older tariff; cover settings, calculation, preview, and confirmation.
- Water billing visibility must not rewrite or claim physical connection state; preserve old false values across initialization.
- Zero water must be deliberate enough to prevent accidental billing while remaining one-step and server-validated.
- Historical PDFs and QR payloads must remain source-faithful and unchanged after repeated output, payments, and conversion.

---

### Task 1: Reconcile branch and establish baseline

**Files:** `.github/workflows/windows.yml`, release-gate documents, existing report.

- [ ] Merge the latest `origin/main` commit into the PR branch after reviewing its complete diff; preserve PR commits.
- [ ] Record SHAs, CI, PR #41/#46 deltas, version, and baseline test count.
- [ ] Run focused baseline reproductions for FX route, inactive tariffs, and legacy water flag without modifying production data.

### Task 2: Financial conversion and tariff eligibility

**Files:** `app.py`, `domain.py`, `db.py`, `config_service.py`, `invoice_pdf.py`; tests for invoice HTTP, tariff services, payments, and PDFs.

- [ ] Add failing HTTP/PDF assertions for USD/CDF water-zero and water-positive fixture invoices, unit prices, subtotals, totals, payments, balances, QR-source invariant, and SQLite snapshot fingerprint.
- [ ] Add failing tests for newest inactive/unconfirmed tariffs, effective dates, and deterministic no-fallback behavior.
- [ ] Implement copy-only one-pass conversion with explicit source rate and presentation currency; reconcile every monetary field and payment while leaving physical quantities and database source unchanged.
- [ ] Implement conservative tariff selection requiring active, confirmed, effective service/type profiles and aligned invoice currency; reject ambiguous or disabled latest entries.

### Task 3: Water connection, zero acknowledgement, and historical compatibility

**Files:** `db.py`, `app.py`, `templates/new_invoice.html`, `static/desktop.js`, migration/HTTP/PDF tests.

- [ ] Add failing migration tests proving `water_enabled=0` survives on old and new records and repeated initialization.
- [ ] Remove universal backfill/triggers and form coercion; keep combined service composition independent of connection state.
- [ ] Add a deliberate zero acknowledgement and server-side confirmation binding; reject missing/stale acknowledgement without reserving an invoice number.
- [ ] Prove old single-service snapshots remain historically faithful; update title selection without changing stored snapshots.

### Task 4: End-to-end PDF, data integrity, and Windows gates

**Files:** focused tests, report tooling/fixtures, CI as needed.

- [ ] Generate route-served synthetic PDFs for USD/CDF and water zero/positive plus a historic invoice; extract and arithmetically reconcile all printed fields.
- [ ] Render images and inspect A4 layout; decode QR locally and confirm source total/no personal data.
- [ ] Run migrations on copied synthetic legacy SQLite fixtures; verify integrity, immutable snapshots/payments, exports, numbering, and no destructive mutations.
- [ ] Run full pytest, compileall, diff checks, CI Windows, and available EXE/MSI/Burn packaging checks; classify unavailable hardware explicitly.

### Task 5: Report and PR readiness

**Files:** `Docs/SEL_RELEASE_GATE_V1/RAPPORT_EXECUTION_CODEX.md`, release-gate matrix, current water report, README if needed.

- [ ] Publish exact commit/test/build evidence and a requirement-to-proof table; refresh outdated claims.
- [ ] Record unresolved SEL decisions, blocked PC-target checks, SafeUpdate production key status, and distinct GO/NO-GO verdicts.
- [ ] Update PR #47 only; leave `main` and releases untouched.
