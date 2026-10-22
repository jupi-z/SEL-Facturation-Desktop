# Plan de développement Codex — phases et dépendances

Toutes les phases sont nécessaires, même si certaines aboutissent à une décision de blocage. Le propriétaire a autorisé l'implémentation future par Codex sur PR séparée; **pas la fusion ni le déploiement immédiat**.

## Phase 0 — Audit et états
- Lire le dépôt entier utile, auditer les routes existantes et les tests; documenter l'état du manifeste `signed:false` et du contrôle Authenticode, la chaîne de build réelle `0.3.24`, le statut de PR #41.
- Fixer le modèle de menace, la migration depuis les anciennes versions et les contrats d'installation Windows; produire un plan court et la table « déjà présent vs à faire ».
- Ne pas casser la facturation ni déplacer le programme dans Program Files.

## Phase 1 — Authentification Ed25519
- Créer le module de vérification du manifeste signé et les tests adversariaux CR-01 à CR-17.
- Ajouter dépendance Ed25519 éprouvée `cryptography` et vérifier son intégration PyInstaller, sans générateur de signature maison.
- Construire un outil de release hors ligne qui assemble le manifeste depuis un fichier existant et signe les **octets définitifs**, avec gestion clé privée protégée. Ajouter un script autonome **verification-only** pour le propriétaire/CI.
- Une **clé publique de production** est requise avant le build initial déployable; Codex peut d'abord utiliser une clé de **test** pour l'E2E. Ne jamais prétendre à un build production tant que la vraie clé propriétaire n'est pas provisionnée.

## Phase 2 — Liaison UI et téléchargement
- Refactoriser `update_client.py`, `update_policy.py`, `app.py`, `templates/updates.html`, `static/desktop.js` : aucune logique de décision non signée; tâche asynchrone; progression réelle; timeout; messages accessibles.
- Distinguer explicitement `windows_code_signed:false` de `manifest_trusted:true` calculé **localement et non accepté tel quel depuis l'API**.
- S'appuyer sur la session authentifiée uniquement pour consentement; jamais comme preuve d'authenticité de l'installateur.

## Phase 3 — Orchestrateur et backup
- Établir un journal de transaction updater séparé des données métier; staging utilisateur protégé, serrure, hash-vérification finale et sauvegarde SQLite vérifiée.
- Construire helper fiable qui attend la fermeture application, lance exclusivement le package validé **sans élévation explicite**, contrôle les codes retour, relance app sous le profil courant si possible.
- Tester gestion du mutex, fichiers verrouillés, WebView2, MSI perUser, ancienne Inno et UAC. Si une invite Windows légitime survient, l'expliquer — ne pas la masquer.
- Mettre en place reprise après panne, et manuel de restauration de l'ancienne version **si** la migration DB le permet.

## Phase 4 — Publication séparée
- Construire les artefacts; auditer les permissions GitHub release; signer le manifeste hors repo et contrôler package bytes, hash, taille, tag, URL; publier la release d'abord, le canal signé ensuite.
- Mode `preview` opt-in contrôlé puis passage `stable` après recettes. Installer une fois manuellement la première version SafeUpdate qui contient la clé publique.
- Ne pas fusionner/publication réelle sans approbation; aucun secret privé dans une PR ou un dépôt public.

## Phase 5 — Recette de niveau métier
- Exécuter la matrice du document `04_MATRICE_TESTS_ET_ACCEPTATION.md`; fournir capture écran de l'UI, logs anonymisés, tests de backup/restore, validation du trajet d'installation et deux updates successives.
- Tester sur les postes SEL si accessibles; préciser ce qui ne l'a pas été.
- Mettre le rapport final `Docs/SEL_SAFEUPDATE_V1/IMPLEMENTATION_REPORT.md`, ouvrir une PR de revue, demander validation explicite du propriétaire.

## Politique de priorité
P0 : fail-closed cryptographique, non-regression DB, installateur perUser réel, récupération, autonomie offline.
P1 : progression, expérience 1 clic, canal stable, nettoyage caches, accessibilité.
P2 : notification périodique avancée, reprise download multi-chunks, éventuelle signature Authenticode future, télémétrie (hors périmètre V1).

## Règles de travail pour Codex
- Une branche `feat/sel-safeupdate-v1` (nom libre unique si occupé); ne pas travailler directement sur `main`.
- Une modification doit avoir au moins un test approprié, pas d'exceptions « temporairement acceptées » sur les contrôles de signature.
- Les modifications concurrentes liées à la facture eau + électricité sont hors périmètre; vérifier les conflits juste avant de préparer PR.
- Faire apparaître en continu progression par phase et progression totale fondées sur critères réellement validés, sans annoncer 100 % si une recette P0 reste en attente.
- Ne pas inventer un certificat Windows ni annoncer l'application « Microsoft approuvée ». La réussite d'un upgrade en contexte per-user ne garantit pas l'absence d'avertissement OS sur tous les postes.
