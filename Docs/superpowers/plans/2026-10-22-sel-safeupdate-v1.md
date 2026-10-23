# SEL SafeUpdate V1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a fail-closed, per-user Windows update path that accepts unsigned Authenticode packages only after Ed25519 manifest authentication and SHA-256 verification, while preserving offline billing and SQLite data.

**Architecture:** Keep the existing Python/Flask/pywebview/SQLite and WiX Burn + per-user MSI stack. Add a small `safeupdate_crypto.py` trust/validation layer for exact manifest bytes, an update transaction/coordinator layer for bounded asynchronous work and verified staging, and a same-user supervisor mode in the native executable for final revalidation, shutdown, installer launch, and relaunch. Production trust remains disabled until SEL provisions a real public key; tests use ephemeral keys only.

**Tech Stack:** Python 3.12, `cryptography` Ed25519, SHA-256, Flask/pywebview, SQLite backup API, PyInstaller, WiX 4, GitHub Actions Windows runner.

**Spec:** `CODEX_SAFEUPDATE_START_HERE.md` and `Docs/SEL_SAFEUPDATE_V1/00_MISSION_CODEX.md` through `06_OPERATIONS_CLES_ET_ROLLOUT.md`.

## Global Constraints

- Executables remain unsigned Authenticode; `windows_code_signed:false` is not a trust signal.
- Only an exact UTF-8 `latest.json` covered by a valid Ed25519 signature and an embedded trusted public key can authorize an update.
- SHA-256 and declared size of the downloaded bootstrapper must match the signed manifest immediately before launch.
- No Authenticode, UAC, SmartScreen, Defender, Smart App Control, or Windows policy bypass is allowed.
- Installation remains per-user through the existing WiX Burn + MSI chain; no `requireAdministrator`, service, Program Files, HKLM, or `shell=True` path.
- Billing remains usable offline; no update failure may mutate invoices, payments, snapshots, exports, or the SQLite schema.
- The first SafeUpdate bootstrap from pre-SafeUpdate clients is manual and must be reported as such.
- No production private key is generated, stored, or referenced in this repository; production publication remains disabled.

## Review Focus

- Raw manifest/signature mismatch, duplicate JSON keys, invalid UTF-8/constants, unknown key id, and modified signed bytes must fail before any offer or download (`tests/test_safeupdate_crypto.py`).
- A valid Ed25519 manifest with `windows_code_signed:false` must be accepted, while an unsigned or Authenticode-only package must be rejected (`tests/test_safeupdate_crypto.py`, `tests/test_update_client.py`).
- A staged package changed after download or a transaction path escaping its private directory must never reach the native bridge (`tests/test_safeupdate_transaction.py`, `tests/test_desktop.py`).
- A replayed sequence, downgrade, expired manifest, stable/preview mismatch, or `min_supported_version` transition must not produce an install action (`tests/test_safeupdate_crypto.py`).
- Backup failure, cancellation, concurrent update requests, offline startup, and installer return codes must preserve normal billing and leave a recoverable transaction (`tests/test_safeupdate_coordinator.py`).

---

### Task 1: Trust root and strict signed-manifest verification

**Files:**
- Create: `safeupdate_crypto.py`
- Create: `safeupdate_keys.py` with an empty production key map and explicit provisioning comments
- Modify: `requirements.txt`, `requirements-build.txt`
- Test: `tests/test_safeupdate_crypto.py`

**Interfaces:**
- `verify_signed_manifest(manifest_bytes: bytes, signature_envelope_bytes: bytes, *, current_version: str, trusted_keys: Mapping[str, str], last_sequence: int = 0, last_manifest_sha256: str | None = None, allowed_channel: str = "stable", now: datetime | None = None) -> dict`
- `parse_strict_json(payload: bytes, *, max_bytes: int) -> object`
- `verify_package(path: Path, *, expected_sha256: str, expected_size: int) -> None`

- [ ] Write failing CR-01 through CR-17 tests with generated ephemeral Ed25519 keys, including exact-byte mutation, duplicate keys, replay, URL safety, unsigned Windows flag, and no-key fail-closed behavior.
- [ ] Run `pytest tests/test_safeupdate_crypto.py -q` and confirm the new tests fail because the module does not exist.
- [ ] Implement strict bounded parsing, detached signature envelope validation, Ed25519 verification, schema/field/type/URL/time/version/channel checks, monotonic sequence checks, and streaming package hash/size verification.
- [ ] Run the crypto tests until green, then run the complete suite.
- [ ] Commit `feat: add fail-closed safeupdate manifest verification`.

### Task 2: Authenticated fetch, transaction staging, and release tooling

**Files:**
- Modify: `update_policy.py`, `update_client.py`
- Create: `update_transaction.py`
- Create: `tools/generate_signed_update.py`, `tools/verify_signed_update.py`, `tools/generate_safeupdate_key.py`
- Test: `tests/test_update_client.py`, `tests/test_safeupdate_transaction.py`, `tests/test_safeupdate_release_tools.py`

**Interfaces:**
- `fetch_manifest(url: str, *, current_version: str, opener=urlopen) -> dict` fetches `latest.json` and sibling `latest.sig.json`, verifies exact bytes before returning an offer.
- `download_package(update: dict, destination: Path, *, current_version: str, progress=None, cancel_event=None, opener=urlopen) -> dict` returns `transaction_id`, `filename`, `sha256`, `version`, and `backup_required` after authenticated staging.
- `load_transaction(transaction_id: str) -> dict`, `verify_transaction(transaction_id: str, *, current_version: str) -> dict`, and `mark_transaction(...)` use only private, atomically-written user staging records.
- Release tools require an explicit external encrypted private-key path, compute package size/hash from the supplied artifact, sign final manifest bytes, and never create keys by default.

- [ ] Replace old Authenticode-only tests with tests for authenticated unsigned packages, bounded progress/cancellation, redirect/scheme rejection, and transaction tamper detection.
- [ ] Run the focused tests red before implementation.
- [ ] Implement sibling signature retrieval with bounded mismatch retries, strict raw-byte verification, private staging, atomic transaction journals, and no execution during download.
- [ ] Implement offline release/signature verification tools and key-generation procedure without running production key generation.
- [ ] Run focused and full tests; inspect that no private-key material appears in tracked files or logs.
- [ ] Commit `feat: stage authenticated safeupdate transactions`.

### Task 3: Asynchronous coordinator and native supervisor

**Files:**
- Create: `safeupdate_coordinator.py`
- Create: `safeupdate_supervisor.py`
- Modify: `app.py`, `desktop.py`
- Test: `tests/test_safeupdate_coordinator.py`, `tests/test_desktop.py`

**Interfaces:**
- `SafeUpdateCoordinator.start_check()`, `.start_download()`, `.cancel()`, and `.snapshot()` expose one serialized operation with states `idle`, `checking`, `available`, `downloading`, `verifying`, `backup`, `ready`, `failed`, `cancelled`, and `recovery_required`.
- `DesktopDocuments.install_update(transaction_id: str) -> dict` accepts only an opaque transaction id and starts the same-user supervisor.
- `safeupdate_supervisor.run(transaction_id: str, parent_pid: int, ...) -> int` waits for the parent, revalidates the transaction, invokes the validated bootstrapper with fixed `/norestart`, records exit state, and relaunches the canonical executable on success.

- [ ] Add failing tests for asynchronous startup/offline behavior, one-operation serialization, real byte progress, backup refusal, supervisor argument hardening, return-code handling, and final startup postcheck.
- [ ] Run the focused tests red.
- [ ] Implement the coordinator as a bounded daemon worker; make startup checks non-blocking and manual checks explicit; keep all UI/session state derived from the coordinator snapshot.
- [ ] Implement pre-install SQLite backup verification and supervisor final revalidation, without elevation or arbitrary command arguments.
- [ ] Run focused and complete tests; verify existing billing flows remain offline-capable.
- [ ] Commit `feat: orchestrate safeupdate installation per user`.

### Task 4: Updates UI, persistence, packaging, and CI

**Files:**
- Modify: `templates/updates.html`, `static/desktop.js`, `static/style.css`, `.github/workflows/windows.yml`, `build-windows.ps1`, `Docs/WINDOWS_CODE_SIGNING.md`
- Test: `tests/test_update_http.py`, `tests/test_ui_branding.py`, `tests/test_ui_shell.py`, `tests/test_packaging_metadata.py`

- [ ] Add failing assertions for authenticated trust wording, unsigned Authenticode distinction, real byte progress, cancel/late/retry controls, and absence of the old “signature Authenticode required” copy.
- [ ] Implement polling of coordinator status, accessible status announcements, consent-only install action, and honest offline/SmartScreen/recovery messages.
- [ ] Add `cryptography` to Windows build dependencies and ensure the PyInstaller app includes SafeUpdate modules without embedding a private key.
- [ ] Update CI to test SafeUpdate and build unsigned artifacts without publishing releases or manifests; keep existing WiX per-user and WebView2 packaging.
- [ ] Run UI/packaging tests and the full suite.
- [ ] Commit `feat: expose safeupdate controls and CI coverage`.

### Task 5: Evidence, report, and PR

**Files:**
- Create: `Docs/SEL_SAFEUPDATE_V1/IMPLEMENTATION_REPORT.md`
- Create: `Docs/SEL_SAFEUPDATE_V1/proofs/` only with redacted test logs/metadata, no keys or business data

- [ ] Run `pytest -q`, `python -m compileall`, `git diff --check`, focused cryptographic adversarial tests, and the local Windows build if available.
- [ ] Run WiX/MSI/Burn packaging verification and inspect artifact hashes; mark Win10/Win11, standard-account, WebView2-absent, SmartScreen, power-loss, and SEL-PC tests explicitly PASS/FAIL/NOT TESTED.
- [ ] Generate only test-key dry-run manifests outside production channels; run independent verification and confirm invalid/tampered artifacts fail.
- [ ] Write the exact test count, commit SHA, transaction/recovery paths, limitations, missing production key, first-bootstrap requirement, and GO/NO-GO status.
- [ ] Request code review, push the dedicated branch, and open a PR against `main` without merging, releasing, or deploying.

## Current audit baseline

- Starting source: `origin/main` at `d5f4eb2cccc76618281da41c2a3e6a3b3f74af03`.
- Baseline suite: `230 passed in 112.68s`.
- Existing updater accepts unsigned JSON decisions and requires Authenticode; it has no Ed25519 root, detached signature, sequence store, transaction journal, async coordinator, or supervisor token.
- Existing public `updates/latest.json` is not a SafeUpdate V1 signed channel and must not be treated as trusted during this implementation.
- Production public key, private key, release approval, real SEL hardware, and Windows 10/11 target recipes are not yet available to this task.
