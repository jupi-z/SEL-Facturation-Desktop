# Facture combinée eau + électricité Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ajouter un contrat de snapshot et un rendu A4 pour une facture unique eau + électricité, avec QR documentaire interne, tout en conservant le parcours électrique et les documents historiques.

**Architecture:** Le moteur de domaine produira un snapshot combiné versionné et indépendant des valeurs SEL non confirmées. Le renderer ReportLab choisira le format combiné uniquement lorsqu’un snapshot explicite le demande ; les anciens snapshots conserveront leur chemin de rendu actuel. Le parcours réel restera en mode électrique tant que SEL n’aura pas validé le mode, tarif et régime fiscal de l’eau ; un choix combiné sera refusé lisiblement au lieu d’inventer une donnée.

**Tech Stack:** Python 3.12, Flask, SQLite, ReportLab vectoriel, pypdf, pytest, pywebview inchangé.

**Spec:** `CODEX_START_HERE.md`, `Docs/SEL_BILLING_COMBINED_V1/01_SPECIFICATION_NORMATIVE.md`, `Docs/SEL_BILLING_COMBINED_V1/02_TESTS_ET_RECETTE.md`, `Docs/SEL_BILLING_COMBINED_V1/04_MAQUETTE_A4.md`

## Global Constraints

- Les calculs monétaires restent en `Decimal` avec `ROUND_HALF_UP` à 0,01.
- Les anciennes factures électriques, snapshots, paiements, exports `sel-facturation-portable` v1 et numéros ne sont jamais recalculés ni réécrits.
- Aucun tarif, volume, taxe, identifiant fiscal, signature ou référence DGI ne doit être inventé.
- Le mode combiné réel est bloqué tant que SEL n’a pas validé le mode eau, le tarif, la devise et les taxes ; les fixtures eau sont exclusivement synthétiques dans les tests.
- Le QR est `SEL_INTERNAL_REFERENCE` v1, local, documentaire, sans données personnelles, URL, secret ou prétention fiscale.
- Aucun changement de dashboard, packaging, mise à jour Windows, authentification, marque ou Monarque Connect.
- Pas de nouvelle dépendance réseau ou QR distante ; ReportLab reste le générateur local.
- La branche sera publiée en PR non fusionnée ; aucune release ne sera créée.

## Review Focus

- Un snapshot historique sans `services` doit produire le même document électrique et ne jamais afficher de section eau — test `test_legacy_invoice_snapshot_keeps_electricity_layout`.
- Un service eau incomplet, négatif, sans tarif ou avec taxe implicite doit être refusé — tests `test_combined_snapshot_rejects_unvalidated_water_service` et `test_combined_snapshot_never_defaults_water_tax`.
- Un QR ne doit exposer que les champs du contrat interne et rester stable après paiement — tests `test_internal_reference_payload_is_minimal_and_stable` et `test_payment_does_not_change_internal_reference_payload`.
- Une conversion CDF combinée doit convertir chaque base et total sans mélanger USD/CDF — test `test_combined_snapshot_conversion_reconciles`.
- Un texte utilisateur long ou contenant des caractères de balisage doit rester échappé et le PDF multipage doit garder ses titres — test `test_combined_pdf_escapes_user_text_and_remains_a4`.

---

### Task 1: Contract de calcul et snapshot combinés

**Files:**
- Modify: `domain.py`
- Test: `tests/test_combined_billing.py`

**Interfaces:**
- Consumes: snapshots électriques existants et deux descriptions de service explicites.
- Produces: `build_combined_snapshot(electricity: dict, water: dict, currency: str) -> dict` et `convert_combined_snapshot(snapshot: dict, exchange_rate: str, quote_currency: str) -> dict`.

- [ ] **Step 1: Write the failing tests** pour deux services, le total `15.02`, taxes ventilées, refus d’eau incomplète/négative, conversion et stabilité du payload interne.
- [ ] **Step 2: Run tests to verify they fail** avec `python -m pytest -q tests/test_combined_billing.py` ; échec attendu sur les fonctions absentes.
- [ ] **Step 3: Implement the additive snapshot contract** dans `domain.py` : `document_kind=COMBINED_WATER_ELECTRICITY`, `schema_version=2`, deux codes obligatoires, unités distinctes, lignes et taxes explicites, validations sans valeurs par défaut.
- [ ] **Step 4: Run focused and regression tests** avec `python -m pytest -q tests/test_combined_billing.py tests/test_consumption_billing.py`.
- [ ] **Step 5: Commit** `feat: ajouter le snapshot combiné eau électricité`.

### Task 2: Rendu A4 et QR interne local

**Files:**
- Modify: `invoice_pdf.py`
- Test: `tests/test_combined_invoice_pdf.py`

**Interfaces:**
- Consumes: snapshot combiné de Task 1 et invoice enregistré avec numéro/date/currency.
- Produces: PDF vectoriel A4 combiné, `internal_reference_payload(invoice: dict) -> str`, sans modifier le rendu des anciens snapshots.

- [ ] **Step 1: Write the failing tests** pour le titre, les deux sections, sous-totaux, total, prix unitaires dans les détails, absence de duplication inutile, payload QR minimal et décodage structurel de la charge.
- [ ] **Step 2: Run tests to verify they fail** avec `python -m pytest -q tests/test_combined_invoice_pdf.py`.
- [ ] **Step 3: Implement the combined renderer** avec ReportLab A4, logo SEL existant, tables vectorielles, taxes par service, paiement/solde, footer/page number et `QrCodeWidget` local ; le renderer legacy reste intact pour les snapshots sans `services`.
- [ ] **Step 4: Run focused PDF tests** avec `python -m pytest -q tests/test_combined_invoice_pdf.py tests/test_invoice_pdf_tariffs.py tests/test_storage_pdf.py`.
- [ ] **Step 5: Commit** `feat: rendre la facture combinee et son qr interne`.

### Task 3: Parcours gardé, prévisualisation et compatibilité d’export

**Files:**
- Modify: `app.py`
- Modify: `templates/new_invoice.html`
- Modify: `templates/invoice_preview.html`
- Modify: `templates/invoice_detail.html`
- Test: `tests/test_combined_invoice_http.py`

**Interfaces:**
- Consumes: `build_combined_snapshot` et `convert_combined_snapshot`.
- Produces: choix explicite dans le formulaire ; le mode combiné refusé sans configuration SEL eau ; affichage générique des services déjà enregistrés ; export CSV/portable lisible pour les snapshots combinés.

- [ ] **Step 1: Write the failing HTTP tests** pour le refus du mode combiné réel, l’affichage d’un snapshot combiné synthétique dans détail/PDF, la conversion CDF et la conservation des clés d’export v1.
- [ ] **Step 2: Run tests to verify they fail** avec `python -m pytest -q tests/test_combined_invoice_http.py`.
- [ ] **Step 3: Implement the guarded input and adapters** : électricité reste le défaut ; eau n’est pas préremplie ; `billing_mode=COMBINED` est bloqué avec message métier ; détail, preview, CSV et conversion n’assument plus les seules clés électriques.
- [ ] **Step 4: Run focused HTTP/export tests** avec `python -m pytest -q tests/test_combined_invoice_http.py tests/test_invoice_preview.py tests/test_http.py tests/test_consumption_http.py`.
- [ ] **Step 5: Commit** `feat: proteger le parcours combine et les exports`.

### Task 4: Fixtures synthétiques, preuves visuelles et rapport de recette

**Files:**
- Create: `tests/fixtures/combined_invoice_cases.py`
- Create: `Docs/audits/2026-10-22-combined-invoice/REPORT.md`
- Create: `Docs/audits/2026-10-22-combined-invoice/samples/` (PDF synthétiques uniquement)
- Test: `tests/test_combined_invoice_visuals.py`

**Interfaces:**
- Consumes: renderer et snapshot des Tasks 1–3.
- Produces: cinq PDFs synthétiques couvrant legacy, combiné simple, taxes eau de test, contenu long/multipage et annulation/paiement partiel, avec contrôles textuels/A4 et rapport des limites.

- [ ] **Step 1: Write the fixture/render test** avec données explicitement synthétiques, sans insertion dans la base SEL réelle.
- [ ] **Step 2: Run it to verify it fails** avant le générateur de samples.
- [ ] **Step 3: Implement the fixture generator and report** ; documenter les décisions SEL manquantes et les contrôles matériels non effectués.
- [ ] **Step 4: Run visual/text/A4 checks** avec `python -m pytest -q tests/test_combined_invoice_visuals.py` et inspecter les pages/tailles produites.
- [ ] **Step 5: Commit** `test: ajouter les preuves synthetiques de facture combinee`.

### Task 5: Vérification finale et PR

**Files:**
- Modify: `Docs/audits/2026-10-22-combined-invoice/REPORT.md`

- [ ] **Step 1: Run full suite and compilation** : `python -m pytest -q` puis `python -m compileall -q .`.
- [ ] **Step 2: Run `git diff --check` and inspect the final diff** ; vérifier migration additive, absence de données eau par défaut et compatibilité d’export.
- [ ] **Step 3: Run the Windows CI on the dedicated PR** ; aucun release/upload de binaire.
- [ ] **Step 4: Complete the report** avec commit de base, commits, tests exacts, preuves PDF, décisions restantes et verdict `GO pilote interne` ou `NO-GO` selon les critères.
- [ ] **Step 5: Open a pull request** vers `main`, non fusionnée, avec le rapport complet.

## Rulings to record during execution

- Le mode combiné réel reste bloqué car le dossier ne fournit aucune règle eau validée ; coût si SEL avait déjà validé ces données ailleurs : activation différée jusqu’à réception d’une preuve métier.
- Le QR est ajouté aux nouveaux snapshots combinés uniquement ; les anciens PDF restent sur leur rendu électrique pour respecter la non-régression de fond.
