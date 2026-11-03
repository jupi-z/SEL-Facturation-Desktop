# SDD ledger — plan: docs/superpowers/plans/2026-11-03-sel-v2-urgent-candidate.md

Pre-flight: plan is executed natively in this session because the user explicitly authorized immediate continuation and no subagent execution method was selected.

Baseline: `caf002533b2f2fcee92af8e63a346890614382ef`, branch `monarque/sel-v2-backend`, PR #58, main `09d9a244b6b243fccdf3313d0a9d1aace2daa071`.

Figma review: file `EuIpbci6IR7m6ggApo5WWm`, read-only session `c3e2d49d-dcfc-43e2-b87e-03a3d31b176a`; core operations, solar/finance, admin/system, invoice documents, and error/accessibility pages were accessible.

Task 1: complete (baseline verified; report updated for `caf0025` and CI `41665371957`)
Task 2: complete.

TDD RED evidence (2026-11-03): commit `dbd1141` added HTTP assertions for local read-side routes. GitHub Actions run `41669026114`, job `126032059472`, concluded `failure` in `cargo test --workspace`; the suite returned HTTP 404 instead of 200 for `GET /local/v1/invoices?limit=20` and `GET /local/v1/solar/sales?limit=20`.

Implementation commits `ee69701`, `6f971f7`, and `63a588d`/`0d4c841` added bounded deterministic SQLite reads and authenticated local routes for invoices, payments, and solar sales. Final GitHub Actions run `41671381091`, job `126040351649`, concluded `success` for fmt, clippy, workspace tests, and PostgreSQL tests. Local execution was not available because `cargo` is absent from the session PATH.

Task 3: implementation complete, validation partial. Commit pending: `desktop_v2.py`, local-only `v2/client/` assets, and `tests/test_desktop_v2.py`. Local Python/JavaScript smoke checks pass (`4 passed`, `py_compile`, `node --check`). Windows WebView2, native window, Rust child process and printer acceptance remain unverified until the Windows workflow/poste pilote runs.

Task 4: packaging prepared, validation pending. `v2/packaging/build-v2.ps1` creates the Rust-backed EXE, per-user MSI, WebView2 bootstrapper and SHA-256 manifest. `.github/workflows/sel-v2-windows.yml` is added; no Windows artifact is claimed before that workflow succeeds.

Task 5: server package prepared, deployment blocked externally. The production Dockerfile, Compose example, TLS reverse-proxy example, init role script, runbook and validation workflow are present. Local Docker Compose interpolation passed; local Docker image build was not executable because the Docker Desktop Linux daemon was unavailable. No real server, secret, certificate, PostgreSQL volume or production key was accessed.
