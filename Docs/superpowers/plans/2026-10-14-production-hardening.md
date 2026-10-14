# SEL Facturation — Plan de durcissement production

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Corriger les risques P0/P1 identifiés pour une exploitation Windows 10/11 plus fiable, sans casser les données existantes ni l’export portable v1.

**Architecture:** Conserver Python/Flask/pywebview/SQLite/PyInstaller. Centraliser les métadonnées de version, corriger WiX 4 avec les primitives valides, ajouter des validations métier et une pagination explicite, puis renforcer sauvegarde et migration additive.

**Tech Stack:** Python 3.12, Flask, SQLite, ReportLab, WiX 4, GitHub Actions Windows, pytest.

**Spec:** `AGENTS.md`, `Docs/CODEX_HANDOFF.md`, `Docs/REVIEW_REPORT.md`.

## Global Constraints

- Préserver `sel-facturation-portable` schema v1 et les snapshots historiques.
- Ne jamais supprimer `%LOCALAPPDATA%\\SEL Facturation` pendant installation, mise à jour ou désinstallation.
- Ne pas annoncer le MSI comme validé avant CI WiX verte et recette Windows cible.
- Toute modification de code doit être précédée d’un test de régression qui échoue.

## Review Focus

- WiX 4 doit compiler le MSI et le bootstrapper avec WebView2 embarqué.
- Une facture existante doit rester lisible après évolution de version et migration SQLite.
- Une échéance invalide, un coefficient extrême ou une référence de paiement manquante doit être rejeté clairement.
- Pagination et recherche ne doivent pas masquer silencieusement les enregistrements au-delà des limites d’affichage.
- Une sauvegarde interrompue ne doit jamais remplacer une sauvegarde valide.

### Task 1: Corriger la chaîne WiX 4

**Files:**
- Modify: `packaging/bundle/Bundle.wxs`
- Modify: `.github/workflows/windows.yml`
- Test: `tests/test_packaging_metadata.py`

- [ ] Ajouter un test vérifiant les éléments WiX 4 valides pour les commandes d’installation/désinstallation et l’absence d’attributs WiX 3.
- [ ] Corriger `ExePackage` et `MsiPackage` avec la syntaxe WiX 4.
- [ ] Compiler en CI et publier les artefacts uniquement sur demande explicite.

### Task 2: Unifier les versions et les sorties

**Files:**
- Create: `version.py`
- Modify: `app.py`, `packaging/SEL-Facturation.iss`, `.github/workflows/windows.yml`, `build-windows.ps1`
- Test: `tests/test_versioning.py`

- [ ] Tester que `/health`, Inno, MSI et bootstrapper utilisent une version cohérente.
- [ ] Remplacer les constantes divergentes par la version centralisée sans migration de données.
- [ ] Vérifier les noms d’artefacts et les chemins de release.

### Task 3: Renforcer les validations métier

**Files:**
- Modify: `app.py`, `domain.py`
- Test: `tests/test_http.py`, `tests/test_domain.py`

- [ ] Rejeter une échéance antérieure à la date d’émission.
- [ ] Rejeter les coefficients hors plage opérationnelle documentée.
- [ ] Exiger une référence pour Banque et Mobile Money.
- [ ] Ajouter des limites explicites aux champs libres sensibles.

### Task 4: Pagination et sûreté des sauvegardes

**Files:**
- Modify: `app.py`, `templates/clients.html`, `templates/invoices.html`, `db.py`
- Test: `tests/test_http.py`, `tests/test_storage_pdf.py`

- [ ] Remplacer les limites silencieuses par une pagination avec page courante et navigation.
- [ ] Ajouter des index SQLite pour les recherches et l’historique.
- [ ] Vérifier qu’une restauration ou migration refuse une base non intègre avant remplacement.
- [ ] Ajouter une version de schéma additive avec migration idempotente.

### Task 5: Documentation et vérification finale

**Files:**
- Modify: `README.md`, `Docs/REVIEW_REPORT.md`, `Docs/MSI_VALIDATION.md`

- [ ] Documenter les versions supportées, la restauration, les limites et le parcours MSI.
- [ ] Exécuter la suite complète, `git diff --check` et les contrôles de packaging.
- [ ] Pousser la branche dédiée et mettre à jour la PR sans publier de release finale.

