# SEL Consumption Billing Rules Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Adapt SEL Facturation Desktop to official monthly consumption billing with controlled Residential/Commercial customer types, fixed USD/kWh rates, 16% VAT, optional CDF presentation, and optional future meter readings without breaking historical data.

**Architecture:** Keep the existing Python/Flask/SQLite, native pywebview/WebView2, and ReportLab stack. Add a tariff catalog and additive snapshots for new invoices; retain legacy progressive-tier and tax fields only for historical compatibility. New invoices use direct kWh input today, while meter readings remain optional future inputs.

**Tech Stack:** Python 3.12, Flask, SQLite, Decimal, ReportLab, pywebview/WebView2, pytest, PyInstaller, WiX/Inno Setup.

**Spec:** `Docs/CODEX_HANDOFF.md` plus the business rules confirmed by SEL in the current conversation (Residential 0.095 USD/kWh HT, Commercial 0.12 USD/kWh HT, VAT 16%, no local tax, USD primary, initial USD/CDF rate 2360, monthly billing, optional meter).

## Global Constraints

- Preserve Python/Flask/SQLite and the native Windows application.
- Migration is additive only; never delete or rewrite clients, invoices, payments, snapshots, or the `sel-facturation-portable` v1 export.
- Existing invoices remain immutable and keep their historical calculations, currencies, taxes, and legacy tiers.
- New invoices use USD, fixed price per kWh, VAT 16%, and no local tax.
- Customer type is controlled: `RESIDENTIAL` or `COMMERCIAL`; no silent default for a new billable customer.
- Monthly billing is represented by `MONTHLY`; the Excel person-count value is not a billing input.
- Meter number and readings are optional until SEL supplies them; direct kWh consumption is the current emission mode.
- CDF output is a presentation variant and must show the USD source amount and applied exchange rate; it must never silently replace USD.
- No DGI certification, QR code, fiscal signature, or unsupported tax identifier is added.
- Work on a dedicated `monarque/` branch, use a PR, run the complete test suite, and verify the Windows build before claiming readiness.

## Review Focus

- A new customer without a type must be blocked from emission, while old records remain readable; test in Task 2 and Task 4.
- A customer without a meter must still be billable from direct kWh consumption; test in Task 3 and Task 5.
- Changing a tariff or exchange rate must not change an issued invoice; test snapshot and regression behavior in Task 2 and Task 4.
- A zero local-tax configuration must remove the local-tax line from new calculations and PDFs without changing old snapshots; test in Task 3 and Task 6.
- USD/CDF conversion must preserve the USD source amount and rate used; test in Task 3 and Task 6.

### Task 1: Freeze the current behavior and establish the implementation branch

**Files:**
- Modify: `Docs/REVIEW_REPORT.md`
- Test: existing `tests/` suite

**Interfaces:**
- Consumes: current schema, legacy calculation behavior, current export format.
- Produces: a reproducible baseline and an explicit list of legacy behaviors that must remain readable.

- [ ] **Step 1: Create a dedicated branch from the current branch tip**

Run:

```powershell
git switch -c monarque/consumption-tariffs
```

- [ ] **Step 2: Install the test dependencies in the project environment**

Run:

```powershell
py -3.12 -m pip install -r requirements.txt -r requirements-build.txt pytest
```

- [ ] **Step 3: Run the existing suite before changing code**

Run:

```powershell
py -3.12 -m pytest -q
```

Expected: record the exact pass/fail count; no new work starts until pre-existing failures are separated from new failures.

- [ ] **Step 4: Document the baseline and legacy compatibility list**

Record the current test result, current version, and the following legacy fields that must remain readable: `tiers`, `local_rate`, `vat_base`, `category`, `client_type`, `meter_number`, invoice snapshots, and portable JSON v1.

- [ ] **Step 5: Commit the audit baseline**

```powershell
git add Docs/REVIEW_REPORT.md
git commit -m "docs: freeze billing migration baseline"
```

### Task 2: Add additive tariff, customer-type, and exchange-rate persistence

**Files:**
- Modify: `db.py`
- Modify: `app.py`
- Test: `tests/test_storage_pdf.py`
- Test: `tests/test_http.py`
- Create: `tests/test_tariff_persistence.py`

**Interfaces:**
- Consumes: existing `settings(c)`, `initialize()`, `clients`, `invoices`, and snapshot storage.
- Produces: additive tables/columns and helpers for `RESIDENTIAL`, `COMMERCIAL`, `MONTHLY`, USD, and the configurable USD/CDF rate.

- [ ] **Step 1: Write failing migration tests**

Add tests asserting that a fresh database contains:

```python
assert tariff_plans == {
    "RESIDENTIAL": {"unit_price": "0.095", "currency": "USD", "unit": "KWH"},
    "COMMERCIAL": {"unit_price": "0.12", "currency": "USD", "unit": "KWH"},
}
assert settings["vat_rate"] == "16"
assert settings["local_rate"] == "0"
assert settings["currency"] == "USD"
assert settings["exchange_rate_usd_cdf"] == "2360"
```

Add a migration test using an old database fixture and assert that old invoices and their snapshots are byte-for-byte unchanged after `initialize()`.

- [ ] **Step 2: Implement additive schema changes in `db.py`**

Add, without dropping existing fields:

```sql
CREATE TABLE IF NOT EXISTS tariff_plans (... customer_type, code, unit, unit_price, currency, vat_rate, effective_from, active ...);
CREATE TABLE IF NOT EXISTS exchange_rates (... base_currency, quote_currency, rate, effective_on, source ...);
```

Add `clients.customer_type` with an empty legacy-safe value rather than silently assigning Residential. Add `billing_frequency` with `MONTHLY` for new records. Keep old `category` and `client_type` columns for compatibility. Store new defaults as settings and seed the two official tariff plans idempotently.

- [ ] **Step 3: Add typed persistence helpers**

Implement helpers with exact responsibilities:

```python
def tariff_plan_for(c, customer_type: str, on_date: str): ...
def list_active_tariff_plans(c): ...
def exchange_rate_for(c, base: str, quote: str, on_date: str): ...
```

They must reject unknown customer types and missing effective rates instead of guessing.

- [ ] **Step 4: Add migration tests for existing data**

Assert that old rows with `category="Résidentiel"`, old progressive `tiers`, local tax, CDF currency, and historical snapshots remain queryable and unchanged.

- [ ] **Step 5: Run focused tests**

Run:

```powershell
py -3.12 -m pytest tests/test_tariff_persistence.py tests/test_storage_pdf.py -q
```

Expected: PASS.

- [ ] **Step 6: Commit the persistence layer**

```powershell
git add db.py app.py tests/test_tariff_persistence.py tests/test_storage_pdf.py tests/test_http.py
git commit -m "feat: add additive SEL tariff and exchange-rate catalogs"
```

### Task 3: Replace the active calculation path with fixed price-per-kWh billing

**Files:**
- Modify: `domain.py`
- Create: `tests/test_consumption_billing.py`
- Modify: `tests/test_domain.py`

**Interfaces:**
- Consumes: Decimal inputs, tariff plans, VAT rate, direct kWh or optional readings.
- Produces: `calculate_flat_rate(...)` and `calculate_from_readings(...)` snapshots with stable decimal strings.

- [ ] **Step 1: Write failing calculation tests**

Cover exact official values:

```python
assert calculate_flat_rate("100", "0.095", "16")["energy"] == "9.50"
assert calculate_flat_rate("100", "0.095", "16")["vat"] == "1.52"
assert calculate_flat_rate("100", "0.095", "16")["total"] == "11.02"
assert calculate_flat_rate("100", "0.12", "16")["total"] == "13.92"
```

Also test zero consumption, decimal kWh, negative input rejection, invalid type, invalid rate, VAT-only taxation, and readings with an optional coefficient.

- [ ] **Step 2: Implement `calculate_flat_rate`**

Use:

```python
def calculate_flat_rate(*, consumption_kwh: str, unit_price: str, vat_rate: str = "16", currency: str = "USD") -> dict:
```

Return `pricing_model="FLAT_KWH"`, one energy line, `local_rate="0"`, `local_tax="0.00"`, VAT basis equal to energy, and all money values as two-decimal strings. Do not use binary floats.

- [ ] **Step 3: Implement optional reading calculation**

Use:

```python
def calculate_from_readings(*, previous: str, current: str, coefficient: str = "1", unit_price: str, vat_rate: str = "16", currency: str = "USD") -> dict:
```

Reject a decreasing reading, but allow the meter fields to be absent in the web flow. Preserve the existing legacy `calculate` function for old tests and historical compatibility.

- [ ] **Step 4: Implement explicit CDF presentation conversion**

Use:

```python
def convert_amount(amount_usd: str, exchange_rate: str, quote_currency: str = "CDF") -> dict:
```

Return the USD source amount, rate, quote currency, and converted amount. Never mutate the original USD calculation.

- [ ] **Step 5: Run focused calculation tests**

Run:

```powershell
py -3.12 -m pytest tests/test_consumption_billing.py tests/test_domain.py -q
```

Expected: PASS.

- [ ] **Step 6: Commit the domain layer**

```powershell
git add domain.py tests/test_consumption_billing.py tests/test_domain.py
git commit -m "feat: calculate fixed kWh tariffs with VAT"
```

### Task 4: Update invoice issuance, snapshots, and portable exports

**Files:**
- Modify: `app.py`
- Modify: `db.py`
- Modify: `tests/test_http.py`
- Modify: `tests/test_invoice_preview.py`
- Create: `tests/test_invoice_tariff_snapshots.py`

**Interfaces:**
- Consumes: client type, monthly frequency, tariff lookup, direct kWh or optional readings.
- Produces: immutable invoice snapshots containing customer type, tariff code, unit price, currency, VAT, and calculation mode.

- [ ] **Step 1: Write failing HTTP tests for direct kWh billing**

Create a Residential and Commercial client without a meter, submit direct consumption, and assert that invoice creation succeeds with the correct totals and `currency="USD"`.

- [ ] **Step 2: Write failing tests for missing/invalid type**

Assert that a client with no controlled type cannot emit a new invoice and receives a clear validation error. Assert that arbitrary values such as `VIP` are rejected.

- [ ] **Step 3: Write failing snapshot immutability tests**

Issue an invoice at 0.095 USD/kWh, change the tariff to another effective plan, and assert that the original `calc_snapshot` and `company_snapshot` remain unchanged.

- [ ] **Step 4: Implement the new invoice flow in `app.py`**

The new flow must:

1. select the client;
2. require a controlled customer type;
3. accept direct kWh consumption;
4. optionally accept readings when provided;
5. retrieve the tariff effective for the billing period;
6. calculate with VAT only;
7. save the tariff and exchange-rate metadata in the snapshot;
8. keep the monthly uniqueness rule;
9. preserve the existing transaction and immutable invoice behavior.

- [ ] **Step 5: Preserve and extend portable JSON v1 safely**

Keep `schema_version: 1` and all existing keys. Add only optional fields where backward-compatible. If a structural v2 becomes necessary, create it separately and continue producing v1 unchanged.

- [ ] **Step 6: Run HTTP and snapshot tests**

Run:

```powershell
py -3.12 -m pytest tests/test_http.py tests/test_invoice_preview.py tests/test_invoice_tariff_snapshots.py -q
```

Expected: PASS.

- [ ] **Step 7: Commit issuance changes**

```powershell
git add app.py db.py tests/test_http.py tests/test_invoice_preview.py tests/test_invoice_tariff_snapshots.py
git commit -m "feat: issue monthly invoices from customer tariff snapshots"
```

### Task 5: Simplify customer and settings interfaces

**Files:**
- Modify: `templates/clients.html`
- Modify: `templates/client_edit.html`
- Modify: `templates/new_invoice.html`
- Modify: `templates/settings.html`
- Modify: `static/app.css`
- Modify: `tests/test_form_accessibility.py`
- Modify: `tests/test_ui_modals.py`
- Create: `tests/test_ui_tariff_rules.py`

**Interfaces:**
- Consumes: controlled customer types, monthly frequency, tariff catalog, and direct consumption route fields.
- Produces: UI that prevents manual tariff entry and does not require a meter.

- [ ] **Step 1: Write failing UI tests**

Assert that:

- the client form contains a select with only Residential and Commercial;
- there is no free-text active category/type field;
- meter input is not required;
- the new invoice form has direct kWh input;
- local tax and progressive tiers are absent from the active settings form;
- USD is the primary displayed currency and CDF is presented as a conversion option.

- [ ] **Step 2: Update customer forms**

Replace free text with a controlled `customer_type` select. Display meter number as optional. Keep contract, address, phone, and service point fields. Show monthly frequency as fixed/read-only for the current scope.

- [ ] **Step 3: Update the invoice form and preview**

Add direct `consumption_kwh`, show the selected type and current unit price, and display VAT 16%. Provide an optional “Afficher en CDF” action that does not modify the USD invoice.

- [ ] **Step 4: Update settings modals**

Keep identity editing separate from billing settings. Replace the old tiers/local-tax editor with tariff cards or a controlled table for the two tariff plans, VAT 16%, USD, and configurable USD/CDF rate. Do not delete legacy settings from the database.

- [ ] **Step 5: Run UI tests**

Run:

```powershell
py -3.12 -m pytest tests/test_form_accessibility.py tests/test_ui_modals.py tests/test_ui_tariff_rules.py tests/test_layout_responsive.py -q
```

Expected: PASS.

- [ ] **Step 6: Commit the UI update**

```powershell
git add templates static tests/test_form_accessibility.py tests/test_ui_modals.py tests/test_ui_tariff_rules.py
git commit -m "feat: streamline customer types and consumption billing UI"
```

### Task 6: Update the PDF and CDF variant without changing historical documents

**Files:**
- Modify: `invoice_pdf.py`
- Modify: `app.py`
- Modify: `templates/invoice_preview.html`
- Modify: `templates/invoice_detail.html`
- Modify: `tests/test_storage_pdf.py`
- Create: `tests/test_invoice_pdf_tariffs.py`

**Interfaces:**
- Consumes: new calculation snapshots and optional conversion data.
- Produces: clean USD PDF and explicit CDF variant, with legacy snapshots still renderable.

- [ ] **Step 1: Write failing PDF tests**

Extract text from a new Residential and Commercial PDF and assert the presence of:

- customer type;
- kWh quantity;
- unit price;
- energy HT;
- TVA 16%;
- total USD;
- no “Taxe locale” line for new invoices.

Render an old legacy invoice and assert that it remains readable with its historical lines.

- [ ] **Step 2: Update the new PDF layout**

Use a single fixed-rate energy line for new invoices. Keep the existing professional header, page footer, payment state, and multi-page behavior. Do not add DGI claims or unsupported fiscal identifiers.

- [ ] **Step 3: Add explicit CDF presentation**

Add a route/action that generates a CDF presentation containing the USD source amount, exchange rate, rate date, and converted amount. It must be clearly labeled as a conversion variant and must not create a second financial invoice record.

- [ ] **Step 4: Run PDF tests and visual checks**

Run:

```powershell
py -3.12 -m pytest tests/test_storage_pdf.py tests/test_invoice_pdf_tariffs.py -q
```

Also render representative PDFs to images and inspect one-page and multi-page output for clipping, overlap, incorrect totals, and wrong currency labels.

- [ ] **Step 5: Commit the PDF update**

```powershell
git add invoice_pdf.py app.py templates/invoice_preview.html templates/invoice_detail.html tests/test_storage_pdf.py tests/test_invoice_pdf_tariffs.py
git commit -m "feat: render SEL fixed-rate USD invoices and CDF variants"
```

### Task 7: Remove active legacy controls while preserving compatibility

**Files:**
- Modify: `db.py`
- Modify: `app.py`
- Modify: `invoice_pdf.py`
- Modify: `README.md`
- Modify: `Docs/CODEX_HANDOFF.md`
- Modify: `tests/test_versioning.py`
- Create: `tests/test_legacy_compatibility.py`

**Interfaces:**
- Consumes: legacy records and new tariff records.
- Produces: explicit separation between active rules and historical compatibility.

- [ ] **Step 1: Write compatibility tests**

Assert that old invoices with progressive tiers, local tax, CDF, and old client fields still open, export, and render exactly from their snapshots.

- [ ] **Step 2: Disable legacy rules for new issuance**

New invoices must reject attempts to use the legacy global tiers or a non-zero local-tax rule. Do not delete the database keys or old data.

- [ ] **Step 3: Remove obsolete user-facing wording**

Remove current wording that says the displayed tariffs are merely an example. Replace it with the confirmed SEL rules. Remove the unused DGI footnote from active settings and exports while leaving historical snapshots untouched.

- [ ] **Step 4: Update documentation**

Document the current rules, optional meter mode, monthly formula, USD/CDF conversion, no local tax, and the distinction between historical legacy invoices and new invoices.

- [ ] **Step 5: Run compatibility tests**

Run:

```powershell
py -3.12 -m pytest tests/test_legacy_compatibility.py tests/test_versioning.py tests/test_storage_pdf.py -q
```

Expected: PASS.

- [ ] **Step 6: Commit compatibility cleanup**

```powershell
git add db.py app.py invoice_pdf.py README.md Docs/CODEX_HANDOFF.md tests/test_legacy_compatibility.py tests/test_versioning.py
git commit -m "refactor: separate active tariff rules from legacy billing data"
```

### Task 8: Full regression, Windows packaging, and release readiness

**Files:**
- Modify: `version.py`
- Modify: `packaging/SEL-Facturation.iss`
- Modify: `packaging/wix/Product.wxs`
- Modify: `build-windows.ps1`
- Modify: `Docs/REVIEW_REPORT.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: completed billing flow and all regression tests.
- Produces: verified branch, Windows build evidence, release notes, and GO/NO-GO decision.

- [ ] **Step 1: Run the complete test suite**

Run:

```powershell
py -3.12 -m pytest -q
```

Expected: all tests pass, including legacy, HTTP, UI, PDF, backup, packaging, and domain tests.

- [ ] **Step 2: Run static and repository checks**

Run:

```powershell
git diff --check
git status --short
```

Confirm no generated database, local secrets, PDFs, installers, or personal data are committed.

- [ ] **Step 3: Build the Windows executable**

Run on Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\build-windows.ps1
```

Verify the EXE starts without Python installed, uses the SEL icon, starts in the native window, creates data outside the install folder, and keeps a single instance.

- [ ] **Step 4: Verify upgrade and data preservation**

Install the previous version in an isolated Windows profile, create a legacy invoice and backup, install the new version with the same AppId/UpgradeCode, and assert:

- only one installed application remains;
- old database and PDFs remain available;
- old invoice snapshots are unchanged;
- new invoices use the new tariff model;
- uninstall does not remove `%LOCALAPPDATA%\SEL Facturation` without an explicit user action.

- [ ] **Step 5: Execute the native end-to-end scenario**

Test:

1. first launch and password setup;
2. configure SEL identity;
3. configure the official tariffs;
4. create a Residential client without a meter;
5. create a Commercial client without a meter;
6. issue one monthly invoice for each;
7. verify USD totals and TVA;
8. produce a CDF variant at 2360;
9. save/view/print PDFs;
10. record a partial payment;
11. export JSON/CSV and SQLite backup;
12. close/restart;
13. verify historical documents;
14. verify no duplicate invoice for the same client/month.

- [ ] **Step 6: Update the audit report**

Record exact commit, test output, build output, Windows version, WebView2 state, PDF viewer/printer result, upgrade result, backup restore result, and any untested hardware. If the SEL target machine was not tested, explicitly mark **pilote non validé sur poste cible**.

- [ ] **Step 7: Commit release documentation and request review**

```powershell
git add version.py packaging build-windows.ps1 Docs/REVIEW_REPORT.md README.md
git commit -m "chore: prepare SEL consumption billing release"
git push -u origin monarque/consumption-tariffs
```

Open a pull request, attach the test/build evidence, and do not publish an installer until the PR and Windows verification are complete.

## Self-review and delivery gate

- The official prices, VAT, monthly frequency, optional meter, USD primary currency, and configurable CDF conversion are covered by Tasks 2–6.
- Historical progressive tiers, local tax, old currencies, old client fields, old snapshots, and portable JSON v1 are preserved by Tasks 2, 4, and 7.
- The plan does not implement Excel import because the provided workbook is a business reference, not a finalized import contract. It can be added later after SEL provides a validated field mapping.
- A GO decision requires the complete test suite, a verified Windows build, an upgrade test, a restore test, and a native Windows/PDF/printer smoke test. Without the SEL target-machine test, the status remains **pilote non validé sur poste cible**.
