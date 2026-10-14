# SEL Facturation — Plan de simplification des interfaces et du parcours utilisateur

> **Pour les agents d’exécution :** utiliser `superpowers:executing-plans` pour implémenter ce plan tâche par tâche, avec un test et un commit à chaque lot.

**Objectif :** rendre SEL Facturation simple à apprendre et fiable à utiliser quotidiennement, en conservant le moteur Python/Flask/SQLite, les données existantes, le format portable `sel-facturation-portable` v1 et les exports destinés à Monarque Connect.

**Architecture :** conserver le shell applicatif connecté et introduire un shell d’authentification séparé pour `/setup` et `/login`. Améliorer ensuite les écrans existants par petites évolutions de templates/CSS, puis sécuriser le parcours de création d’un décompte par une étape de prévisualisation avant émission immuable. Aucun remplacement de pile ni migration destructive.

**Technologies :** Python 3.12, Flask, Jinja2, SQLite, HTML/CSS/JavaScript local, pywebview/WebView2, pytest, PyInstaller, Inno Setup et WiX.

**Spécification :** demande utilisateur du 15/10/2026 et audit des interfaces existantes dans `templates/`, `static/app.css`, `static/desktop.js` et `app.py`.

## Contraintes globales

- Ne pas remplacer la pile Python/Flask/SQLite.
- Migration additive uniquement ; ne jamais modifier les snapshots de factures existants.
- Conserver l’export JSON `sel-facturation-portable` v1.
- Ne jamais présenter un document comme authentifié par la DGI.
- L’application doit rester native Windows et hors ligne, sans ouverture systématique du navigateur.
- Toute émission de facture doit rester déterministe, décimale et immuable après confirmation.
- Les changements seront réalisés sur une branche dédiée avec PR, tests locaux et CI Windows.
- Ne pas annoncer un build ou un parcours comme validé sans preuve reproductible.

## Points de revue prioritaires

- Accès à `/login` et `/setup` sans barre latérale vide ni shell connecté : test HTTP des deux états et vérification visuelle native Windows.
- Double soumission ou rafraîchissement de l’étape de confirmation d’un décompte : une seule facture active doit être créée.
- Erreur de calcul ou index invalide dans la prévisualisation : aucune facture ni séquence ne doit être consommée.
- Annulation accidentelle et paiements : confirmation explicite, règles métier inchangées, historique conservé.
- Petit écran, clavier et lecteur d’écran : navigation, focus, messages d’erreur et tableaux restent utilisables.

## Découpage de livraison

### Tâche 1 — Shell d’authentification sans card

**Fichiers :**
- Modifier : `templates/base.html`
- Modifier : `templates/login.html`
- Modifier : `templates/setup.html`
- Modifier : `static/app.css`
- Tester : `tests/test_http.py`

**Interface produite :**
- `base.html` choisit un shell `auth-shell` pour les pages non authentifiées et le shell `app-shell` pour les pages connectées.
- `login.html` et `setup.html` utilisent la même composition visuelle, sans `.panel` englobant le formulaire.
- Les deux pages exposent un titre, une zone de marque SEL, un formulaire centré par l’espace et un statut local/hors ligne.

- [ ] Ajouter des tests de rendu vérifiant l’absence de `side` et de `top` connectés sur `/login` et `/setup`.
- [ ] Ajouter les éléments d’accessibilité : `main`, titre unique, `aria-live` pour les erreurs, identifiants de champs, labels explicites et focus initial.
- [ ] Implémenter le layout auth responsive : logo/identité SEL, formulaire libre sans card, bouton principal, pied de page version/données locales.
- [ ] Ajouter l’affichage/masquage du mot de passe sans dépendance externe et sans divulgation de la valeur.
- [ ] Vérifier que les erreurs de connexion et de confirmation restent visibles après une soumission invalide.
- [ ] Exécuter `pytest tests/test_http.py -q` et faire une capture native sur Windows 10/11 avant validation.
- [ ] Commit : `feat: simplify authentication screens`.

### Tâche 2 — Navigation et vocabulaire cohérents

**Fichiers :**
- Modifier : `templates/base.html`
- Modifier : `templates/dashboard.html`
- Modifier : `templates/invoices.html`
- Modifier : `templates/new_invoice.html`
- Modifier : `templates/clients.html`
- Modifier : `templates/settings.html`
- Modifier : `static/app.css`
- Tester : `tests/test_http.py`, nouveau `tests/test_ui_copy.py`

**Interface produite :**
- Terminologie stable : `Abonnés`, `Compteurs`, `Décomptes`, `Paiements`, `Paramètres de facturation`.
- Navigation active avec `aria-current="page"` et état visuel suffisamment contrasté.
- Les actions principales sont identifiables par écran ; les exports sont regroupés sous une section données/sauvegardes.

- [ ] Écrire des tests vérifiant les libellés principaux et l’état actif sur chaque route protégée.
- [ ] Ajouter un helper Jinja ou une condition locale pour marquer le lien courant sans dupliquer la logique métier.
- [ ] Harmoniser les titres `h1`, `h2`, textes de boutons et messages sans changer les URLs ni les noms de routes.
- [ ] Améliorer la navigation clavier et les styles `:focus-visible`.
- [ ] Vérifier l’affichage à 900 px, 1240 px et 1440 px de largeur.
- [ ] Commit : `refactor: clarify application navigation`.

### Tâche 3 — Tableau de bord orienté action

**Fichiers :**
- Modifier : `app.py:157-169`
- Modifier : `templates/dashboard.html`
- Modifier : `static/app.css`
- Tester : `tests/test_http.py`, nouveau `tests/test_dashboard_view.py`

**Interface produite :**
- Le tableau de bord affiche clairement abonnés, décomptes actifs, montant émis, encaissé et solde.
- Un bloc de statut indique si le barème est configuré et si une sauvegarde locale existe.
- Les libellés ne prétendent pas mesurer autre chose que ce que les requêtes calculent réellement.

- [ ] Écrire les tests pour factures émises, annulées, paiements partiels et aucune donnée.
- [ ] Exposer dans la vue des valeurs explicites : `issued_total`, `paid_total`, `balance_total`, `configuration_status`, `backup_status`.
- [ ] Corriger les textes et les unités sans modifier les snapshots ni les calculs.
- [ ] Ajouter des actions contextuelles : `Créer un décompte`, `Ajouter un abonné`, `Voir l’historique`.
- [ ] Commit : `feat: make dashboard action-oriented`.

### Tâche 4 — Formulaires abonnés et paramètres lisibles

**Fichiers :**
- Modifier : `templates/clients.html`
- Modifier : `templates/client_edit.html`
- Modifier : `templates/settings.html`
- Modifier : `static/app.css`
- Modifier si nécessaire : `app.py:171-243`
- Tester : `tests/test_http.py`, nouveau `tests/test_form_accessibility.py`

**Interface produite :**
- Les formulaires sont regroupés par tâche et non par ordre technique.
- Les champs obligatoires, unités et erreurs sont explicites.
- Les paramètres d’entreprise et de tarification sont visuellement séparés.

- [ ] Ajouter des titres de groupes et textes d’aide courts pour identité, compteur, contrat, calcul et coordonnées.
- [ ] Ajouter des `id`/`for`, `aria-describedby` et messages d’erreur locaux sans supprimer les validations serveur.
- [ ] Ajouter une validation UX des doublons de compteur avant enregistrement, sans changer les données historiques.
- [ ] Ajouter un bouton de réinitialisation des recherches et des états vides pédagogiques.
- [ ] Vérifier que les modifications d’un abonné ne changent jamais le snapshot d’un décompte existant.
- [ ] Commit : `refactor: simplify subscriber and settings forms`.

### Tâche 5 — Création de décompte en trois étapes

**Fichiers :**
- Modifier : `app.py:277-312`
- Modifier : `templates/new_invoice.html`
- Créer : `templates/invoice_preview.html`
- Modifier : `static/app.css`
- Modifier si nécessaire : `static/desktop.js`
- Tester : `tests/test_http.py`, `tests/test_domain.py`, nouveau `tests/test_invoice_preview.py`

**Interface produite :**
- Étape 1 : saisie des abonné, période, index, frais et échéance.
- Étape 2 : calcul affiché sans écriture en base.
- Étape 3 : confirmation explicite avant création immuable.
- La confirmation utilise un jeton de formulaire serveur signé ou un état temporaire côté session, sans exposer de données sensibles.

- [ ] Écrire les tests garantissant qu’une prévisualisation valide ne crée aucune facture et ne modifie pas `sequences`.
- [ ] Écrire les tests d’index inversé, barème incomplet, période déjà facturée et double confirmation.
- [ ] Extraire une fonction interne pure pour préparer le contexte de prévisualisation à partir des mêmes fonctions de domaine existantes.
- [ ] Ajouter une route POST de confirmation qui recalcule côté serveur puis écrit une seule fois sous transaction `BEGIN IMMEDIATE`.
- [ ] Présenter consommation, tranches, taxes, frais, total, devise et échéance avant émission.
- [ ] Ajouter un bouton de retour à la saisie et un bouton `Émettre définitivement le décompte`.
- [ ] Vérifier que le rafraîchissement ou le retour arrière ne crée pas de doublon.
- [ ] Commit : `feat: add invoice review before issuance`.

### Tâche 6 — Détail, paiements, PDF et actions sensibles

**Fichiers :**
- Modifier : `templates/invoice_detail.html`
- Modifier : `static/desktop.js`
- Modifier : `static/app.css`
- Modifier si nécessaire : `app.py:314-376`
- Tester : `tests/test_http.py`, `tests/test_desktop.py`, nouveau `tests/test_invoice_actions.py`

**Interface produite :**
- Le total, le solde, le statut et les actions PDF sont visibles en premier.
- Les statuts `ENREGISTRÉ`, `ANNULÉ`, `PAYÉ`, `PARTIEL` et `IMPAYÉ` sont distingués.
- L’annulation exige une confirmation native et reste impossible si un paiement existe.
- Les actions PDF affichent un résultat de succès ou un fallback d’impression compréhensible.

- [ ] Ajouter les tests des états sans paiement, paiement partiel, paiement complet et facture annulée.
- [ ] Ajouter confirmation avant annulation et conserver le motif obligatoire.
- [ ] Ajouter un feedback non bloquant après ouverture, enregistrement ou impression du PDF ; réserver les alertes aux erreurs nécessitant l’attention.
- [ ] Ajouter `aria-busy`, désactivation temporaire et restauration du libellé autour des actions PDF.
- [ ] Vérifier que le nom du PDF et son emplacement restent compatibles avec Windows et les données existantes.
- [ ] Commit : `feat: clarify invoice actions and payment states`.

### Tâche 7 — Accessibilité, états vides et erreurs applicatives

**Fichiers :**
- Modifier : `templates/base.html`
- Créer : `templates/error.html` ou pages spécialisées 400/404/500
- Modifier : `static/app.css`
- Modifier : `app.py`
- Tester : nouveau `tests/test_error_pages.py`, `tests/test_accessibility_contract.py`

- [ ] Ajouter des pages d’erreur SEL sans trace technique exposée à l’utilisateur.
- [ ] Ajouter titres de tableaux, `caption`, `scope`, régions live et focus après navigation/erreur.
- [ ] Vérifier les contrastes, tailles de cible, clavier et zoom à 125 %.
- [ ] Documenter les limites qui nécessitent une validation manuelle avec lecteur d’écran et poste SEL.
- [ ] Commit : `feat: polish error and accessibility states`.

### Tâche 8 — Recette complète et livraison

**Fichiers :**
- Modifier : `Docs/REVIEW_REPORT.md`
- Modifier : `README.md` si les parcours utilisateur ou instructions changent
- Modifier : `Docs/CODEX_HANDOFF.md` si nécessaire

- [ ] Exécuter `pytest -q` et `python -m compileall -q .`.
- [ ] Générer un PDF avec une ligne, plusieurs lignes et débordement multipage ; vérifier visuellement le résultat.
- [ ] Tester setup, login, erreur de mot de passe, navigation, abonné, paramètres, prévisualisation, émission, paiement, annulation, export et sauvegarde.
- [ ] Tester le démarrage natif WebView2 et l’absence d’ouverture systématique du navigateur.
- [ ] Compiler l’EXE PyInstaller, le Setup Inno et le MSI/bootstrapper WiX sur CI Windows.
- [ ] Installer le MSI sur une machine de test Windows 10 et Windows 11, dans un profil vierge, puis vérifier la séparation programme/données.
- [ ] Vérifier la conservation des données sur mise à jour et la restauration d’une sauvegarde.
- [ ] Produire les captures d’écran de chaque étape sur le poste cible SEL ; signaler explicitement toute recette non effectuée.
- [ ] Mettre à jour le rapport avec preuves, limites et avis GO/NO-GO.
- [ ] Demander une revue de PR puis fusionner uniquement si tous les contrôles obligatoires sont verts.

## Critères de simplicité à appliquer partout

- Une page = une décision principale.
- Une action principale clairement visible.
- Les termes métier restent identiques d’un écran à l’autre.
- Les erreurs apparaissent près de l’action qui les a provoquées.
- Toute action irréversible demande une confirmation lisible.
- Le système explique ce qui va être conservé, calculé ou imprimé.
- L’utilisateur n’a jamais besoin de comprendre SQLite, Flask, WebView2 ou Monarque Connect pour utiliser SEL.

## Résultat attendu

Le résultat sera une évolution progressive de l’interface, pas une reconstruction : même moteur de calcul, mêmes données, mêmes exports et mêmes garanties d’immuabilité, avec un parcours plus guidé, plus lisible et moins sujet aux erreurs humaines.
