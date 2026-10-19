# Gestion CRUD complète SEL — Plan d’implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Compléter les parcours de gestion nécessaires dans SEL Facturation, rendre tous les champs abonnés modifiables lorsqu’ils sont pertinents, et préserver l’immutabilité des décomptes et paiements historiques.

**Architecture:** Conserver Python/Flask/SQLite/pywebview. Ajouter des migrations SQLite additives, centraliser les règles de cycle de vie dans des helpers de service, puis exposer des routes Flask protégées et des onglets de paramètres cohérents. Les suppressions destructives seront limitées aux abonnés jamais utilisés ; les données historiques seront archivées, annulées ou corrigées par opérations métier.

**Tech Stack:** Python 3.11/3.12, Flask, SQLite, Jinja, JavaScript natif, pytest, CI Windows/WiX.

**Spec:** `Docs/CODEX_HANDOFF.md` et l’audit CRUD validé par la demande utilisateur du 19/10/2026.

## Global Constraints

- Migration SQLite additive uniquement ; aucune facture, paiement ou snapshot historique ne doit être supprimé ou recalculé.
- Export JSON `sel-facturation-portable` schema v1 conservé.
- Le moteur de facturation et la fenêtre native Python/Flask/SQLite restent en place.
- Les factures et paiements restent immuables ; correction par annulation/contre-écriture, pas par suppression directe.
- Toutes les modifications passent par branche dédiée, tests, PR et CI Windows.

## Review Focus

- Un numéro de contrat, centre, point de service et coordonnées modifiés doivent être persistés et visibles après rechargement.
- Un abonné déjà utilisé par une facture ne doit jamais être supprimé physiquement ; son archivage doit le retirer des nouvelles factures.
- Un type, une devise, un mode de paiement ou un tarif désactivé ne doit plus être proposé aux nouvelles opérations, mais doit rester lisible dans l’historique.
- Une nouvelle version de tarif ou de taux datée ne doit pas modifier les documents déjà émis.
- Toute configuration modifiée doit rester transactionnelle et migrable depuis une base existante.

### Task 1: Tests CRUD et cycle de vie

**Files:**
- Create: `tests/test_crud_management.py`
- Test: `tests/test_form_drafts.py`, `tests/test_ui_tariff_rules.py` si les attentes existantes doivent évoluer

- [x] Écrire les tests rouges pour la modification complète d’un abonné, l’archivage/restauration et la suppression sûre.
- [x] Écrire les tests rouges des catalogues types, devises, tarifs, taux, taxes et modes de paiement.
- [x] Écrire les tests rouges de visibilité historique et de refus des opérations invalides.
- [x] Exécuter les tests ciblés et constater les échecs attendus.

### Task 2: Migrations et services de données

**Files:**
- Modify: `db.py`
- Modify: `config_service.py`
- Test: `tests/test_crud_management.py`, `tests/test_config_service.py`

- [x] Ajouter les colonnes additives de cycle de vie des abonnés et les index nécessaires.
- [x] Ajouter les lectures complètes des catalogues et les helpers transactionnels de mise à jour/activation.
- [x] Préserver les déclencheurs d’immutabilité et les exports existants.
- [x] Exécuter les tests de migration et de persistance.

### Task 3: Routes protégées et règles métier

**Files:**
- Modify: `app.py`
- Modify: `domain.py`
- Test: `tests/test_crud_management.py`

- [x] Persister tous les champs visibles de la fiche abonné, notamment le numéro de contrat.
- [x] Ajouter archive/restauration et suppression physique uniquement pour un abonné sans dépendances.
- [x] Ajouter les opérations de gestion des catalogues et l’historique consultable.
- [x] Rejeter toute modification destructive d’une facture ou d’un paiement immuable.

### Task 4: Interfaces et parcours

**Files:**
- Modify: `templates/client_edit.html`, `templates/clients.html`, `templates/settings.html`, `templates/invoice_detail.html`
- Modify: `static/desktop.js`, `static/app.css`
- Test: `tests/test_form_accessibility.py`, `tests/test_ui_navigation.py`, `tests/test_ui_tariff_rules.py`

- [x] Afficher clairement tous les champs métier modifiables de l’abonné, avec sections et brouillons conservés.
- [x] Ajouter les actions d’archivage/restauration et les confirmations adaptées.
- [x] Ajouter les onglets de gestion des catalogues sans exposer les champs compteur actuellement masqués.
- [x] Afficher les statuts actifs/inactifs sans perdre les références historiques.

### Task 5: Vérification et livraison

**Files:**
- Modify: `README.md`, `Docs/REVIEW_REPORT.md`

- [x] Exécuter la suite complète pytest et les contrôles statiques.
- [ ] Exécuter la CI Windows et vérifier le build WiX.
- [x] Incrémenter la version uniquement pour la release générée après validation des tests.
- [ ] Fournir le nouvel installateur en signalant la signature et la recette sur le poste SEL.
