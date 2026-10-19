# SEL UX First-Use and Forms Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the local SEL billing application self-explanatory for a first-time operator and more efficient for daily work without changing billing rules, stored data, or the native desktop stack.

**Architecture:** Keep Flask/Jinja, SQLite, pywebview/WebView2 and the current flat SEL visual layer. Improve the existing routes and templates with explicit state-aware calls to action, durable empty states, responsive settings navigation, and modal scroll behavior. Keep all business validation in the existing Python routes; template changes only explain or route the user to the next valid action.

**Tech Stack:** Python, Flask, Jinja2, vanilla JavaScript, CSS, pytest, PyInstaller/WiX.

**Spec:** The approved audit and user direction in the conversation: the system must be simple without training, reduce wasted space and confusion, preserve the SEL identity, keep forms usable after errors/refresh, and make common paths pleasant.

## Global Constraints

- Preserve the Python/Flask/SQLite/pywebview/WebView2 stack.
- Preserve invoice snapshots and the `sel-facturation-portable` v1 JSON format.
- Migration remains additive only; this plan does not alter the schema or billing calculations.
- Do not introduce DGI, conformity, legal-warning, signature, QR, or fictional fiscal copy.
- Keep the meter/coefficient fields hidden while meter mode is inactive.
- Keep the application local and do not open a browser as part of normal operation.
- Increment the application version for the releasable build.

## Review Focus

- A fresh installation with no validated tariff configuration must be guided to settings before invoice creation.
- A configured installation with no subscribers must be guided to the subscriber creation flow rather than showing a dead disabled control.
- Empty directories and invoice history must explain what is empty and offer the next action.
- Settings tabs and subscriber modal must remain usable at narrow viewport widths without clipped controls or competing scroll containers.
- Draft persistence and all existing endpoint names, field names, methods, and invoice calculations must remain unchanged.

### Task 1: First-use guardrails

**Files:**
- Modify: `templates/dashboard.html`
- Modify: `templates/new_invoice.html`
- Test: `tests/test_ui_first_use.py`

**Interfaces:**
- Consumes: existing `stats['configured']`, `configured`, and `clients` template values.
- Produces: state-aware dashboard and invoice creation CTAs with accessible explanations.

- [ ] Add failing tests for unconfigured dashboard CTA, unconfigured invoice guidance, and no-subscriber invoice guidance.
- [ ] Run the focused tests and observe failures against the current markup.
- [ ] Render a clear primary route to settings when configuration is incomplete; retain subscriber creation as a secondary path.
- [ ] Render a clear primary route to the subscriber directory when no subscribers exist; keep the preview button disabled only with an associated explanation.
- [ ] Run the focused tests and confirm they pass.
- [ ] Commit the first-use guardrails.

### Task 2: Empty states and action semantics

**Files:**
- Modify: `templates/clients.html`
- Modify: `templates/dashboard.html`
- Modify: `templates/invoices.html`
- Modify: `templates/invoice_detail.html`
- Modify: `templates/client_edit.html`
- Modify: `static/brand.css`
- Test: `tests/test_ui_empty_states.py`

**Interfaces:**
- Consumes: existing endpoint URLs and current empty-loop branches.
- Produces: reusable `.empty-state` content and explicit action groups without changing CRUD behavior.

- [ ] Add failing tests requiring actionable empty states and semantic table scopes on directory/history tables.
- [ ] Run the focused tests and observe the expected failures.
- [ ] Replace bare empty table cells with compact explanatory blocks and contextual links.
- [ ] Normalize lifecycle and detail-page action groups so destructive actions remain visually separated and responsive.
- [ ] Add the shared empty-state styling and table header scope where missing.
- [ ] Run the focused tests and the complete suite.
- [ ] Commit the empty-state and action semantics.

### Task 3: Settings and modal responsive usability

**Files:**
- Modify: `templates/settings.html`
- Modify: `templates/clients.html`
- Modify: `static/app.css`
- Modify: `static/brand.css`
- Test: `tests/test_ui_responsive_flows.py`

**Interfaces:**
- Consumes: existing settings tab hashes, modal focus trap, and draft persistence.
- Produces: wrapped settings tabs and a single predictable scroll region for long forms.

- [ ] Add failing tests for wrapped settings tabs and modal action/scroll rules at narrow widths.
- [ ] Run the focused tests and observe the expected failures.
- [ ] Make settings tabs wrap at narrow widths while preserving hash navigation and keyboard tab semantics.
- [ ] Make the modal form the sole scroll container, keep its action row visible, and preserve focus restoration.
- [ ] Run the focused tests and the complete suite.
- [ ] Commit responsive usability changes.

### Task 4: Release preparation and verification

**Files:**
- Modify: `version.py`
- Modify: `packaging/SEL-Facturation.iss`
- Modify: `README.md`
- Modify: `tests/test_versioning.py`

**Interfaces:**
- Consumes: completed UI changes and current build scripts.
- Produces: a versioned release candidate suitable for the existing CI/build pipeline.

- [ ] Change the application version from `0.3.14` to `0.3.15` and add/update the failing version assertions first.
- [ ] Run the focused version test and observe the expected failure.
- [ ] Update the canonical version references while preserving the existing packaging flow.
- [ ] Run the full pytest suite, static copy checks, and Windows build script.
- [ ] Review the diff for unchanged business logic, no forbidden legal/DGI copy, and no data/schema changes.
- [ ] Commit the release preparation.

