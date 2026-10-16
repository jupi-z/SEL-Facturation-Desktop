# Official SEL Tariffs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make SEL billing use only the official current model: residential and commercial prices per kWh in USD, 16% VAT, no local tax, and optional CDF conversion.

**Architecture:** Keep Python/Flask/SQLite and the existing immutable invoice snapshots. Replace the fragile single-active-tariff behavior with date-versioned tariff plans whose historical rows remain queryable. Remove obsolete tariff inputs from the settings path and make current-period billing fail only when the selected period truly has no tariff.

**Tech Stack:** Python 3.12, Flask, SQLite, Decimal, pytest, HTML/CSS, PyInstaller/WiX.

## Global Constraints

- Official residential price: `0.095 USD/kWh`.
- Official commercial price: `0.12 USD/kWh`.
- VAT is fixed at `16%`; local tax is `0%`.
- Base currency is USD; CDF is an optional converted PDF variant using a configurable USD/CDF rate.
- Existing invoice snapshots remain immutable.
- No Python/Flask/SQLite replacement and no destructive data reset.

## Review Focus

- A tariff update on the 17th must still allow billing the month beginning on the 1st.
- A prior billing month must resolve the tariff version effective at that period.
- A future billing month must resolve the newest tariff effective before that month.
- Residential and commercial prices must never be swapped.
- Obsolete tier/local-tax inputs must not alter the official flat-per-kWh calculation.

### Task 1: Tariff version lookup and persistence

**Files:** `db.py`, `app.py`, `tests/test_tariff_persistence.py`, `tests/test_consumption_http.py`

- [x] Add failing tests proving date-versioned lookup remains valid after a tariff update and that current-month billing works after a mid-month settings save.
- [x] Change lookup to select the latest enabled tariff effective on or before the billing period without disabling historical versions.
- [x] Make tariff settings create/update the official version without making earlier periods unresolvable.
- [x] Run targeted tests, then the full suite.

### Task 2: Official settings model and validation

**Files:** `app.py`, `templates/settings.html`, `tests/test_tariff_persistence.py`, `tests/test_form_accessibility.py`

- [x] Add failing tests for fixed VAT/local-tax behavior and the official two-price settings surface.
- [x] Remove obsolete editable tier/local-tax behavior from the official settings path while retaining safe compatibility for stored invoice snapshots.
- [x] Keep USD as the base currency and the CDF exchange rate configurable.
- [x] Run targeted tests and verify settings edits do not erase SEL identity.

### Task 3: User-facing diagnostics and calculation regression coverage

**Files:** `app.py`, `templates/new_invoice.html`, `tests/test_consumption_http.py`, `tests/test_consumption_billing.py`

- [x] Add a regression test for the exact error scenario and a clear error that identifies type and billing period when no tariff exists.
- [x] Ensure direct consumption always uses the selected customer type’s official rate and 16% VAT with zero local tax.
- [x] Run all tests and verify no invoice is inserted on failed preview.

### Task 4: Windows verification and release artifact

**Files:** `version.py`, packaging metadata as required

- [x] Run full tests, rebuild the Windows EXE and WiX bootstrapper, and verify the generated package version and included assets.
- [x] Record any non-blocking build warnings without declaring unsupported validation.
- [x] Commit and push the dedicated branch/PR.
