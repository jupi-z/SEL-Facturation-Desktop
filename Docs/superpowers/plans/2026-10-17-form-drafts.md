# Conservation des brouillons de formulaires — Plan d’implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Conserver automatiquement les saisies non sensibles après une erreur de validation, une redirection ou une actualisation, puis les effacer après succès.

**Architecture:** Utiliser la session Flask existante comme stockage temporaire par parcours, avec des clés de brouillon séparées pour éviter les collisions entre écrans. Les templates reçoivent les valeurs du brouillon avec repli vers les données persistées. Les secrets d’authentification ne seront jamais enregistrés dans un brouillon.

**Tech Stack:** Python 3.12, Flask, session signée existante, Jinja, SQLite, pytest, pywebview/packaging Windows inchangés.

**Spec:** `Docs/audits/2026-10-17-invoice-entry/REPORT.md` et demande utilisateur du 17 octobre 2026.

## Global Constraints

- Migration SQLite additive uniquement ; aucune donnée métier ou facture historique ne doit être supprimée.
- Conserver la pile Python/Flask/SQLite/pywebview et l’export JSON `sel-facturation-portable` v1.
- Ne jamais stocker les mots de passe de setup ou de connexion dans une session de brouillon.
- Les factures émises restent immuables et le brouillon ne doit pas modifier le calcul déjà prévisualisé.
- Toute nouvelle version livrable doit être vérifiée par la suite de tests et le build Windows local.

## Review Focus

- Une saisie de consommation invalide doit être retrouvée après actualisation, sans créer de facture.
- Un changement de paramètres refusé doit retrouver le bon onglet et toutes ses valeurs, sans écraser la configuration persistée.
- Une création d’abonné dans une modale doit rouvrir la modale avec ses valeurs après erreur.
- Un paiement excessif ou une annulation sans motif doit préserver respectivement le paiement ou le motif.
- Un succès doit supprimer le brouillon correspondant ; les mots de passe doivent rester absents de la session.

### Task 1: Brouillon du nouveau décompte

**Files:**
- Modify: `app.py:417-451`
- Modify: `templates/new_invoice.html`
- Test: `tests/test_form_drafts.py::test_new_invoice_preserves_invalid_submission_after_refresh`

- [ ] Écrire le test de régression (fait) et vérifier son échec.
- [ ] Enregistrer uniquement les champs de saisie dans `session['invoice_form']` avant validation.
- [ ] Réutiliser `invoice_form` ou `invoice_draft` au GET et effacer `invoice_form` après prévisualisation valide.
- [ ] Conserver `invoice_draft` jusqu’à émission réussie, afin qu’une erreur à la confirmation ne perde pas la saisie.
- [ ] Préremplir sélection, période, consommation, échéance et observation dans Jinja.
- [ ] Exécuter le test ciblé puis la suite.

### Task 2: Brouillons des abonnés

**Files:**
- Modify: `app.py:218-271`
- Modify: `templates/clients.html`
- Modify: `templates/client_edit.html`
- Modify: `static/desktop.js`
- Test: `tests/test_form_drafts.py::test_client_create_reopens_preserved_draft_after_refresh`
- Test: `tests/test_form_drafts.py::test_client_edit_preserves_invalid_submission_after_refresh`

- [ ] Stocker les champs non sensibles de création sous `session['client_form']` en cas d’échec.
- [ ] Préremplir la modale de création et lui ajouter un état d’auto-ouverture lorsqu’un brouillon existe.
- [ ] Ajouter dans le gestionnaire modal l’ouverture automatique accessible après chargement, sans modifier le comportement des autres modales.
- [ ] Stocker les champs d’édition sous une clé dédiée au client et fusionner le brouillon avec la fiche persistée.
- [ ] Effacer chaque brouillon après enregistrement réussi.
- [ ] Exécuter les deux tests ciblés.

### Task 3: Brouillons des paramètres

**Files:**
- Modify: `app.py:273-383`
- Modify: `templates/settings.html`
- Test: `tests/test_form_drafts.py::test_settings_preserves_invalid_identity_after_refresh_and_keeps_tab`

- [ ] Stocker un dictionnaire par section (`identity`, `tariffs`, `customer_type`, `operations`) sans CSRF.
- [ ] Faire revenir un GET sans query sur le premier onglet possédant un brouillon.
- [ ] Préremplir chaque champ de chaque onglet à partir du brouillon avec repli vers la base.
- [ ] Effacer uniquement la section validée après succès, sans toucher aux autres brouillons.
- [ ] Vérifier que l’onglet actif, le hash et la redirection existants restent cohérents.
- [ ] Exécuter le test ciblé.

### Task 4: Paiement et annulation

**Files:**
- Modify: `app.py:507-565`
- Modify: `templates/invoice_detail.html`
- Test: `tests/test_form_drafts.py::test_payment_and_void_forms_preserve_invalid_submissions_after_refresh`

- [ ] Stocker séparément le paiement et le motif d’annulation par identifiant de facture en cas d’échec.
- [ ] Passer ces valeurs au rendu du détail et préremplir les champs correspondants.
- [ ] Effacer le brouillon associé après succès.
- [ ] Exécuter le test ciblé et vérifier qu’aucun paiement ni changement de statut n’est créé en cas d’erreur.

### Task 5: Nettoyage, version et livraison

**Files:**
- Modify: `version.py`
- Modify: `packaging/SEL-Facturation.iss`
- Modify: `tests/test_versioning.py`
- Modify: `tests/test_update_http.py`
- Modify: `Docs/REVIEW_REPORT.md`

- [ ] Incrémenter la version patch pour cette amélioration utilisateur.
- [ ] Exécuter `pytest -q`, `node --check static/desktop.js` et `git diff --check`.
- [ ] Exécuter `build-windows.ps1` et vérifier l’EXE ainsi que le bootstrapper versionné.
- [ ] Mettre à jour le compte rendu avec les tests et le statut pilote.
- [ ] Committer et pousser la branche dédiée.
