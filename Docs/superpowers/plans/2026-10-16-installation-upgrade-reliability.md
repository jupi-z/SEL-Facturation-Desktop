# Installation and Upgrade Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make Windows installation, same-version repair, upgrade, downgrade prevention, and data preservation deterministic for SEL Facturation Desktop.

**Architecture:** WiX Burn is the only supported delivery package. The MSI remains per-user and installs only program files; `%LOCALAPPDATA%\SEL Facturation` remains the data root. A small update-policy module will validate signed/versioned packages before launching the bootstrapper, while SQLite backup and integrity checks remain the safety boundary.

**Tech Stack:** Python 3.12, Flask, SQLite, PyInstaller, WiX 4, WebView2 Evergreen Standalone, PowerShell tests on Windows.

**Spec:** `Docs/REVIEW_REPORT.md` and `Docs/CODEX_HANDOFF.md`.

## Global Constraints

- Preserve the Python/Flask/SQLite stack.
- Never delete `%LOCALAPPDATA%\\SEL Facturation` during install, update, repair, or uninstall.
- Preserve `sel-facturation-portable` schema version 1 and historical invoice snapshots.
- WiX Burn is the only supported release installer; Inno Setup is legacy compatibility tooling only.
- A downgrade must be rejected; a same-version install must repair or clearly report that the version is already installed.
- A higher-version install must create and verify a SQLite backup before replacing program files.
- No unsigned package may be presented as a production release.

## Review Focus

- Existing Inno installation followed by WiX installation must be detected or explicitly blocked rather than silently creating ambiguity.
- Same-version launch must not create a second uninstall entry or overwrite user data.
- Downgrade attempts must fail before program files or data are changed.
- Interrupted update must leave a verified backup and a recoverable installation.
- A manifest with an invalid hash, unsupported version, or non-HTTPS URL must be rejected.

---

### Task 1: Establish one packaging version source

**Files:**
- Modify: `version.py`, `packaging/SEL-Facturation.iss`, `.github/workflows/windows.yml`, `build-windows.ps1`
- Test: `tests/test_versioning.py`, `tests/test_packaging_metadata.py`

- [ ] Add failing tests proving the application, MSI, bundle, and generated filenames use `APP_VERSION` and that Inno is marked legacy.
- [ ] Run the focused tests and confirm the failures identify version drift.
- [ ] Implement generated packaging arguments and remove hard-coded release filenames from the supported path.
- [ ] Re-run focused tests and commit.

### Task 2: Make WiX lifecycle behavior explicit

**Files:**
- Modify: `packaging/wix/Product.wxs`, `packaging/bundle/Bundle.wxs`
- Test: `tests/test_packaging_metadata.py`

- [ ] Add tests for stable upgrade identity, major upgrade, same-version repair semantics, downgrade prevention, and preserved per-user data location.
- [ ] Run them red.
- [ ] Add explicit WiX conditions and package metadata for repair/upgrade/downgrade behavior, WebView2 detection, and one supported installation path.
- [ ] Compile MSI and bootstrapper locally, then run metadata tests and commit.

### Task 3: Add safe update-policy validation

**Files:**
- Create: `update_policy.py`
- Modify: `app.py`, `requirements.txt`
- Test: `tests/test_update_policy.py`, `tests/test_update_http.py`

- [ ] Test accepted HTTPS manifests, rejected HTTP manifests, rejected invalid hashes, rejected downgrades, and same-version no-op behavior.
- [ ] Run the tests red.
- [ ] Implement pure validation of `{version, url, sha256, package_type}` without downloading or installing from a request handler.
- [ ] Expose only status/check information in the local UI; installation remains delegated to the signed WiX bootstrapper.
- [ ] Run focused tests and commit.

### Task 4: Protect data across update and recovery

**Files:**
- Modify: `db.py`, `desktop.py`, `README.md`, `Docs/REVIEW_REPORT.md`
- Test: `tests/test_storage_pdf.py`, `tests/test_update_policy.py`

- [ ] Add tests for verified pre-update backup, integrity-check failure, and recovery instructions.
- [ ] Run them red.
- [ ] Add an explicit pre-update backup API and an update lock that refuses to proceed while SQLite is not healthy.
- [ ] Document the supported recovery path and the fact that uninstall does not remove business data.
- [ ] Run focused tests and commit.

### Task 5: Windows packaging and upgrade matrix

**Files:**
- Modify: `.github/workflows/windows.yml`, `Docs/CODEX_HANDOFF.md`
- Create: `tests/test_windows_packaging_matrix.py`

- [ ] Add static matrix tests for clean install, same version, upgrade, downgrade, WebView2 present/absent, and legacy Inno detection policy.
- [ ] Run them red.
- [ ] Make the workflow build only the supported bootstrapper and publish MSI/EXE metadata with hashes; keep signing mandatory for release publication.
- [ ] Run all tests, build EXE/MSI/bootstrapper, perform administrative extraction, and document any target-PC-only validation.
- [ ] Commit and update the PR.

### Task 6: Final verification

- [ ] Run the full suite in bounded groups and `git diff --check`.
- [ ] Rebuild the EXE, MSI, and WebView2 bootstrapper from the final commit.
- [ ] Run an isolated EXE smoke test and verify database, session key, log, and backup creation.
- [ ] Review the final diff and update the audit with evidence and remaining external gates.
