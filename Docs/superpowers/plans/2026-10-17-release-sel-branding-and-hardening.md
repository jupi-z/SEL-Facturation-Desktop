# SEL Facturation Release Branding and Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a version 0.3.8 candidate that keeps SEL as the customer-facing identity, adds legally accurate developer attribution, protects snapshots, and hardens the Windows release path.

**Architecture:** Keep the existing Python/Flask/SQLite/pywebview/ReportLab stack. Add a small legal/branding surface that is separate from customer documents, isolate backup retention by file class, and tighten installer/update metadata without deleting the SEL data root.

**Tech Stack:** Python 3.12, Flask/Jinja2, SQLite, HTML/CSS/JavaScript, pytest, PyInstaller, WiX 4, ReportLab.

**Spec:** `docs/superpowers/specs/2026-10-17-release-sel-branding-and-hardening.md`

## Global Constraints

- SEL remains the dominant identity in screens, PDFs and customer documents.
- MonarQue Group and LTEX logos appear only in the legal/about surface; the footer may say `Designed and secured by LTEX · MonarQue Group`.
- No developer logo is placed in the customer invoice PDF.
- Migrations are additive and the `sel-facturation-portable` v1 export remains compatible.
- WiX Burn is the only supported official installer.
- Version candidate is `0.3.8`.
- Signing and target-machine validation remain explicit external gates.

## Review Focus

- A long legal footer must not shift or clip the SEL navigation at 900px width; test the responsive shell.
- An export or pre-update snapshot must survive daily retention pruning; test mixed backup classes.
- A same-version, upgrade, downgrade and legacy-installation preflight must make distinct decisions; test the matrix.
- An update manifest with a valid hash but invalid publisher must be rejected; test the signature gate.
- A generated release tag must match `APP_VERSION`; test the workflow metadata statically.

### Task 1: Brand assets and legal/about surface

**Files:**
- Create: `static/brand/monarque-group-horizontal.png`
- Create: `static/brand/ltex.png`
- Create: `templates/about.html`
- Modify: `templates/base.html`
- Modify: `static/brand.css`
- Modify: `app.py`
- Modify: `tests/test_ui_branding.py`
- Create: `tests/test_legal_surface.py`

**Interfaces:**
- Produces a protected route `about()` at `/about`.
- Produces template context constants for SEL identity, developer attribution and public legal fields.

- [ ] **Step 1: Write failing tests**
  - Assert `/about` is authenticated.
  - Assert the page contains `SOCIÉTÉ D’ÉNERGIE DU LUALABA`, `MonarQue Group S.A.S.`, `LTEX`, the official RCCM and the official contact email.
  - Assert the base layout contains the discreet footer copy and does not add developer logos to invoice templates.
- [ ] **Step 2: Run the focused tests and verify the expected failures**
  - Run `pytest tests/test_legal_surface.py tests/test_ui_branding.py -q`.
- [ ] **Step 3: Copy and wire the supplied transparent PNG assets**
  - Preserve the source files without cropping or recoloring.
  - Keep the SEL logo as the only application identity in the sidebar and authentication panel.
- [ ] **Step 4: Implement the protected about route and template**
  - Keep legal copy factual and separate from SEL operational configuration.
  - Add a small footer link from the authenticated shell and a non-invasive attribution footer in the authentication shell.
- [ ] **Step 5: Add the minimum CSS for a responsive legal surface**
  - Use a flat layout consistent with the existing visual system and keep the logos small, contained and accessible.
- [ ] **Step 6: Run focused tests and the responsive/static UI tests**
  - Run `pytest tests/test_legal_surface.py tests/test_ui_branding.py tests/test_layout_responsive.py -q`.
- [ ] **Step 7: Commit**
  - Commit as `feat: add SEL legal and developer attribution surface`.

### Task 2: Safe backup classes and snapshot retention

**Files:**
- Modify: `db.py:300-327`
- Modify: `app.py:209-213`
- Modify: `tests/test_storage_pdf.py`

**Interfaces:**
- Produces `prune_daily_backups(folder: Path, retention: int) -> None` that only considers `sel-YYYY-MM-DD.sqlite3` files.
- Keeps `pre_update_backup()` and manual `backup()` behavior unchanged apart from classification-safe pruning.

- [ ] **Step 1: Write a failing mixed-class retention test**
  - Create daily, `sel-preupdate-*`, and `sel-export-*` files; run the daily backup with retention 1; assert only old daily files are deleted.
- [ ] **Step 2: Run the test and verify it fails**
  - Run `pytest tests/test_storage_pdf.py::test_daily_retention_preserves_preupdate_and_export_snapshots -q`.
- [ ] **Step 3: Implement strict daily filename matching**
  - Sort only exact daily snapshot names; never glob all `sel-*.sqlite3` for deletion.
- [ ] **Step 4: Keep dashboard listing behavior explicit**
  - Ensure the most recent backup label can still show a daily or pre-update snapshot without changing the storage contract.
- [ ] **Step 5: Run storage and full regression tests**
  - Run `pytest tests/test_storage_pdf.py tests/test_http.py tests/test_invoice_actions.py -q`.
- [ ] **Step 6: Commit**
  - Commit as `fix: protect non-daily SQLite snapshots from retention`.

### Task 3: Installer version matrix and release metadata

**Files:**
- Modify: `installer_version.py`
- Modify: `packaging/preflight_launcher.py`
- Modify: `packaging/bundle/Bundle.wxs`
- Modify: `.github/workflows/windows.yml`
- Modify: `tests/test_installer_version_matrix.py`
- Modify: `tests/test_packaging_metadata.py`

**Interfaces:**
- Keeps `discover_installations() -> list[dict[str, Any]]` and `choose_preflight_action(...) -> str` backward-compatible.
- Adds a controlled local installation marker only if needed; its schema must identify the canonical data root and executable path without storing secrets.

- [ ] **Step 1: Add failing tests for release tag derivation and the supported installation matrix**
  - Assert the workflow does not contain a fixed historical release tag.
  - Assert clean, same-version, upgrade, downgrade and legacy cases map to their documented actions.
  - Assert the marker cannot cause a downgrade or data deletion.
- [ ] **Step 2: Run focused tests and verify the failures**
  - Run `pytest tests/test_installer_version_matrix.py tests/test_packaging_metadata.py -q`.
- [ ] **Step 3: Implement version-derived release metadata**
  - Derive the release tag and artifact paths from `APP_VERSION`.
  - Keep publication blocked unless all artifacts are Authenticode-valid.
- [ ] **Step 4: Harden the supported portable-marker behavior**
  - Detect only the marker owned by SEL in the expected local data root.
  - Do not claim to detect arbitrary copies that have never run.
- [ ] **Step 5: Review WiX registry detection and document the runtime matrix**
  - Preserve the verified legacy cleanup condition and add tests/documentation for WebView2 registry variants rather than guessing at runtime behavior.
- [ ] **Step 6: Run packaging tests and static checks**
  - Run `pytest tests/test_installer_version_matrix.py tests/test_packaging_metadata.py tests/test_installer_preflight.py -q`.
- [ ] **Step 7: Commit**
  - Commit as `fix: make installer decisions and release tags version driven`.

### Task 4: Update client security boundary

**Files:**
- Modify: `update_client.py`
- Modify: `update_policy.py`
- Modify: `app.py`
- Modify: `tests/test_update_client.py`
- Modify: `tests/test_update_http.py`

**Interfaces:**
- Keeps manifest fetching and staged-package APIs compatible.
- Publisher validation must fail closed and remain separate from download/hash validation.

- [ ] **Step 1: Add a failing exact-publisher verification test**
  - A certificate subject containing the expected string as a substring must not be accepted as the expected publisher.
- [ ] **Step 2: Run the test and verify it fails**
  - Run `pytest tests/test_update_client.py -q`.
- [ ] **Step 3: Implement strict normalized publisher matching**
  - Compare the expected publisher against the complete normalized subject or a configured certificate identity; never accept a substring match.
- [ ] **Step 4: Ensure the application remains safe when update configuration is absent**
  - The status endpoint remains informational and never executes a package.
- [ ] **Step 5: Run focused update tests**
  - Run `pytest tests/test_update_client.py tests/test_update_http.py tests/test_update_policy.py -q`.
- [ ] **Step 6: Commit**
  - Commit as `fix: fail closed on updater publisher validation`.

### Task 5: Version, documentation and production package

**Files:**
- Modify: `version.py`
- Modify: `packaging/SEL-Facturation.iss`
- Modify: `README.md`
- Modify: `Docs/CODEX_HANDOFF.md`
- Modify: `Docs/WINDOWS_CODE_SIGNING.md`
- Modify: `Docs/REVIEW_REPORT.md`
- Modify: `tests/test_versioning.py`

- [ ] **Step 1: Add a failing version-consistency test for 0.3.8**
  - Assert application, package metadata and generated filenames use the same version.
- [ ] **Step 2: Run the focused test and verify the expected failure**
  - Run `pytest tests/test_versioning.py -q`.
- [ ] **Step 3: Bump and document version 0.3.8**
  - Mark unsigned and unvalidated status clearly where signing or target validation is still pending.
  - Remove stale statements that describe Inno Setup as the supported installer.
- [ ] **Step 4: Run documentation/static packaging tests**
  - Run `pytest tests/test_versioning.py tests/test_packaging_metadata.py -q`.
- [ ] **Step 5: Commit**
  - Commit as `chore: prepare SEL release 0.3.8`.

### Task 6: Full verification and release candidate build

**Files:**
- Modify: `Docs/REVIEW_REPORT.md` only for final evidence.

- [ ] **Step 1: Run the complete test suite**
  - Run `pytest -q` and record the exact count and exit code.
- [ ] **Step 2: Run static verification**
  - Run `python -m compileall -q app.py config_service.py db.py desktop.py domain.py installer_version.py invoice_pdf.py update_client.py update_policy.py packaging`.
  - Run `node --check static/desktop.js`.
  - Run `git diff --check`.
- [ ] **Step 3: Build the Windows candidate**
  - Run `powershell -ExecutionPolicy Bypass -File .\build-windows.ps1` and require exit code 0.
- [ ] **Step 4: Verify artifacts**
  - Confirm EXE, MSI and bootstrapper exist, have version 0.3.8, and record SHA-256 hashes and Authenticode status.
- [ ] **Step 5: Record release gates**
  - State clearly whether signing, clean-install testing, upgrade testing, backup restore and target-machine validation were completed.
- [ ] **Step 6: Commit the final evidence**
  - Commit as `docs: record SEL 0.3.8 release candidate evidence`.
