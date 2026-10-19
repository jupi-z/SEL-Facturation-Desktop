# SEL UI Shell Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Improve the shared SEL interface shell and action layouts so forms remain readable, actions are correctly placed, the sidebar stays visible while scrolling, update status is shown only on the update screen, and every logged-in page carries a subtle linked editor footer.

**Architecture:** Keep the existing Flask/Jinja, native pywebview, Python and SQLite stack. Make the behavior change in the shared layout and desktop JavaScript, and make the visual change in the existing CSS token layer so all pages inherit the same rules. Refactor only template action groups that currently depend on stretched form defaults.

**Tech Stack:** Python, Flask, Jinja2, vanilla JavaScript, CSS, pytest.

**Spec:** The approved audit and user request in the conversation: remove the repeated “Vérification indisponible” section from ordinary pages; make the sidebar non-mobile on desktop; add the exact footer copy `Application développée et sécurisée par LTEX · MonarQue Group`, with only `MonarQue Group` linked to `https://monarquegrp.com/`; optimize button placement and preserve the SEL visual identity.

## Global Constraints

- Preserve the Python/Flask/SQLite/pywebview stack.
- Preserve invoice snapshots and the `sel-facturation-portable` v1 JSON format.
- Do not alter billing calculations, tax behavior, update policy, or database schema.
- Keep the application local and do not open the browser as part of normal operation.
- Keep the footer small, visible, and non-invasive; do not add legal or DGI wording.
- Keep the update check available on the dedicated updates screen, but do not render its unavailable state on every authenticated page.

## Review Focus

- A logged-in page must not contain the global update status markup or trigger the update poll merely because it was opened.
- The dedicated updates page must retain its explicit update controls and status behavior.
- The sidebar must remain usable and scrollable when its own content exceeds the viewport.
- Action groups must remain keyboard-accessible and wrap safely at narrow widths.
- The editor footer must render exactly once per logged-in page and link only the MonarQue Group text.

### Task 1: Shared shell behavior and footer

**Files:**
- Modify: `templates/base.html`
- Modify: `static/desktop.js`
- Modify: `static/brand.css`
- Test: `tests/test_ui_shell.py`

**Interfaces:**
- Consumes: existing logged-in base layout and update page route.
- Produces: logged-in pages without global update status markup, a shared `.app-footer`, and a desktop sticky sidebar.

- [ ] Write tests asserting the dashboard response has no `[data-update-status]`, has one exact footer, and links `MonarQue Group` to `https://monarquegrp.com/`.
- [ ] Run the focused tests and observe the expected failures against the current layout.
- [ ] Remove the global statusbar from the authenticated base layout; add the footer after the logged-in main content.
- [ ] Limit the update polling script to pages that explicitly provide an update status node; preserve the dedicated updates screen.
- [ ] Add desktop sticky-sidebar, responsive fallback, footer, and footer-link styles in the brand layer.
- [ ] Run the focused tests and confirm they pass.
- [ ] Commit the shared-shell changes.

### Task 2: Action-group and form layout normalization

**Files:**
- Modify: `templates/client_edit.html`
- Modify: `templates/dashboard.html`
- Modify: `templates/invoice_detail.html`
- Modify: `templates/new_invoice.html`
- Modify: `templates/settings.html`
- Modify: `templates/updates.html`
- Modify: `static/brand.css`
- Test: `tests/test_ui_shell.py`

**Interfaces:**
- Consumes: shared `.app-footer`, `.side`, and action styles from Task 1.
- Produces: explicit `.page-actions`, `.form-actions`, `.lifecycle-actions`, and `.danger-zone` structures with responsive wrapping.

- [ ] Add focused template assertions for client edit actions and the main invoice/settings/update submit areas.
- [ ] Run the focused assertions and observe failures for the current stretched/default layouts.
- [ ] Refactor action markup without changing endpoint names, HTTP methods, field names, or business logic.
- [ ] Add shared action-group rules that prevent full-width stretching except for intentional primary form controls, while preserving narrow-screen wrapping.
- [ ] Run focused tests and then the complete pytest suite.
- [ ] Commit the action layout changes.

### Task 3: Documentation, audit, and delivery verification

**Files:**
- Modify: `README.md` only if the interface behavior or update-screen wording is documented there.
- Test: `tests/test_ui_shell.py`, full `tests/` suite.

**Interfaces:**
- Consumes: completed shell and template changes from Tasks 1–2.
- Produces: verified branch ready for review and release integration.

- [ ] Run static searches proving no ordinary base template still emits `Vérification indisponible`.
- [ ] Run the complete test suite and record the count.
- [ ] Run the Windows build/release verification available in the repository.
- [ ] Review the diff for unchanged business logic and no accidental legal/DGI copy.
- [ ] Commit any documentation-only adjustment if required.
- [ ] Prepare a concise delivery report including test/build status and the target-hardware validation status.
