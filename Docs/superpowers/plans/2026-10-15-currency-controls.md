# Currency Controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remplacer la devise libre par un référentiel SEL contrôlé, ajouter des taux de change datés et capturer les informations nécessaires dans les nouvelles factures sans modifier les historiques.

**Architecture:** Conserver SQLite, Flask, le moteur `domain.py` et l’export portable v1. Ajouter un catalogue de devises et un historique de taux par migrations additives. Les calculs restent déterministes : les tarifs sont calculés dans la devise de référence CDF, puis convertis et snapshotés dans la devise d’émission.

**Tech Stack:** Python 3.12, Flask, SQLite, Decimal, Jinja, pytest, ReportLab.

**Spec:** User request dated 2026-10-15 and `Docs/CODEX_HANDOFF.md`.

## Global Constraints

- Migration SQLite additive uniquement.
- Les factures et snapshots déjà émis restent immuables.
- L’export `sel-facturation-portable` `schema_version: 1` reste compatible.
- Aucun taux récupéré automatiquement sur Internet dans cette version.
- Les devises réellement autorisées et les taux applicables doivent être confirmés par SEL.
- Toute modification reste sur une branche dédiée puis PR.

## Review Focus

- Une devise arbitraire comme `ABC` doit être refusée par le serveur et l’interface.
- Une devise différente de CDF doit exiger un taux strictement positif et daté.
- Une modification de taux ne doit pas modifier une facture déjà émise.
- Le tableau de bord ne doit jamais additionner implicitement CDF et USD.
- Une ancienne base sans catalogue doit migrer sans perte et produire un export v1 lisible.

### Task 1: Domain currency and conversion primitives

**Files:** `domain.py`, `tests/test_domain.py`

- [x] Ajouter les tests rouges pour le catalogue autorisé, la validation d’un taux et la conversion déterministe.
- [x] Implémenter les primitives Decimal minimales et vérifier le calcul existant.
- [ ] Exécuter les tests ciblés puis la suite domaine.

### Task 2: Additive SQLite catalog and rate history

**Files:** `db.py`, `tests/test_storage_pdf.py` ou nouveau `tests/test_currency_storage.py`

- [x] Ajouter les tests rouges de migration, de catalogue par défaut et de taux effectif.
- [x] Ajouter les tables `currency_catalog` et `currency_rates`, sans toucher aux snapshots historiques.
- [ ] Vérifier l’intégrité SQLite et la compatibilité d’une base existante.

### Task 3: Settings endpoint and invoice snapshots

**Files:** `app.py`, `tests/test_http.py`, `tests/test_currency_flow.py`

- [x] Ajouter les tests rouges de sélection, de validation et d’émission CDF/USD.
- [x] Rendre la devise contrôlée côté serveur, enregistrer le taux/date/source et snapshotter les informations.
- [ ] Préserver les factures précédentes lors d’un changement de configuration.

### Task 4: Settings UI, invoice display and dashboard safety

**Files:** `templates/settings.html`, `templates/invoice_preview.html`, `templates/invoice_detail.html`, `templates/dashboard.html`, `invoice_pdf.py`, tests UI/PDF

- [x] Ajouter les tests rouges de présence de liste, taux conditionnel et affichage de devise.
- [x] Séparer « Devise et taux » du barème/taxes et expliciter l’effet futur uniquement.
- [x] Afficher les taux et devises dans les documents sans prétention fiscale.

### Task 5: Portable export, documentation and regression verification

**Files:** `app.py`, `README.md`, `Docs/CODEX_HANDOFF.md`, tests export

- [x] Ajouter les tests rouges de compatibilité export v1 et champs optionnels de change.
- [x] Documenter les règles d’arrondi, la devise de référence CDF et la validation SEL.
- [ ] Exécuter toute la suite, `git diff --check`, puis compiler Windows si l’outillage est disponible.
