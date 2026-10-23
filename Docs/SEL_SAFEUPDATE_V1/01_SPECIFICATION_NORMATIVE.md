# SEL SafeUpdate V1 — Spécification normative

**Statut :** exigences **MUST / SHOULD / MUST NOT** pour Codex, validées conceptuellement par le propriétaire.  
**Périmètre :** application de bureau Python/Flask/pywebview locale, packaging PyInstaller + WiX Burn/MSI **par utilisateur**, SQLite hors dossier d'installation.  
**Compatibilité :** Windows 10/11 x64 pris en charge sous réserve de test; politiques de sécurité des postes SEL non connues.

## A. Expérience utilisateur
- Vérifier au démarrage une fois, **en tâche indépendante** et avec timeout, puis périodiquement suivant une fréquence raisonnable avec jitter; ne pas bloquer le lancement, la connexion, la facturation ou l'impression. Autoriser la vérification manuelle dans Mises à jour.
- Si Internet est indisponible, travailler normalement et afficher « Vérification impossible — le logiciel reste utilisable hors connexion » au bon endroit, sans pop-up répétitive.
- Notifier seulement une **version supérieure**, appartenant au canal approuvé (stable / preview opt-in), et non une downgrade / version identique.
- Afficher version, taille, principales modifications, provenance, état « paquet vérifié par SEL » **seulement après validation Ed25519**, et état de sauvegarde ; ne **jamais** afficher « Microsoft/Windows certifié » pour un EXE unsigned.
- Bouton « Mettre à jour » = **consentement explicite**. Après ce clic, télécharger, vérifier, contrôler la disponibilité métier, sauvegarder, fermer SEL, installer et relancer automatiquement si OS et paquet le permettent. Si une étape bloque, annuler sans toucher aux données et fournir des instructions claires.
- Afficher la progression réelle de téléchargement par octets reçus, étapes de vérification et d'installation distinctes ; ne jamais fabriquer des pourcentages d'installation.
- « Plus tard », « Annuler » avant installation et rapport d'erreur non technique accessibles ; aucune mise à jour forcée à distance dans V1.
- Une seule opération d'update par installation; les requêtes concurrentes sont idempotentes; protéger la route par authentification locale et CSRF, mais la confiance du contenu vient **uniquement** de la signature Ed25519, jamais de la session Flask.

## B. Plateforme et privilèges
- **Ne pas demander elevation au démarrage** et ne jamais ajouter un manifest `requireAdministrator` au programme. Ne jamais modifier ou désactiver UAC, Defender, SmartScreen, Smart App Control, politiques d'entreprise ou `AlwaysInstallElevated`.
- Maintenir **Scope=perUser** du MSI WiX et l'installation dans LocalAppData; aucune écriture dans Program Files/HKLM ni installation de service permanent dans le chemin nominal.
- Le runtime WebView2 doit être détecté et, si absent, installé selon son mode per-user lorsque possible. Les cas nécessitant des privilèges ou bloqués par la politique Windows sont des **limitations explicites**.
- Utiliser exclusivement la chaîne **WiX Burn + preflight + MSI** actuelle pour les upgrades; éviter double installation MSI/Inno; conserver la détection des anciennes installations Inno et le refus de downgrade.
- Un package Windows non signé peut déclencher SmartScreen ou être bloqué; **ne pas garantir zéro dialogue Windows**. Fournir un état de blocage honnête et des instructions officielles, jamais un bypass automatisé.

## C. Authentification de mise à jour — impératif
- Les nouveaux clients SafeUpdate embarquent une **clé publique Ed25519 authentique et épinglée**; la clé privée correspondante n'est jamais expédiée ni enregistrée dans le dépôt. Génération hors dépôt, stockage chiffré hors ligne et reprise opérationnelle documentée.
- Vérifier la **signature détachée sur les octets exacts** du manifeste `latest.json` avant de lire son contenu comme source de décision. Refuser absence de signature, mauvais key-id, mauvaise signature, doublons JSON, schéma inattendu, champs incohérents, version inférieure, non-HTTPS, expiration et régression de numéro de séquence.
- Le manifeste signé contient `sha256`, `size_bytes`, `version`, `channel`, `product`, `package_type`, `release_url`, `url`, `sequence`, `created_at`, `expires_at`, `notes` et `min_supported_version` selon le schéma versionné. Refuser des champs critiques inconnus, longueurs hors limites ou types invalides.
- Le hash SHA-256 n'est valide que **s'il est couvert par la signature Ed25519**. Après téléchargement entier dans un dossier utilisateur privé, comparer taille et hash, relire le manifeste authentifié associé à cette transaction et revalider **immédiatement avant lancement**.
- Les préversions Authenticode absentes sont supportées quand et seulement quand **le manifeste est signé Ed25519 et le fichier correspond exactement aux bytes autorisés**. `signed:false` représente seulement l'absence de **signature Windows**, pas un statut de confiance applicative.
- Préparer un futur mode Authenticode optionnel : quand un certificat sera disponible, exiger *les deux* signatures, ou gérer explicitement une politique versionnée. **Ne jamais** rendre Authenticode obligatoire pour l'usage actuel du canal SafeUpdate V1.
- Ne jamais créer une clé d'acceptation factice dans les tests de production, une clé privée dans la CI hors environnement secrets contrôlé ou une désactivation globale de validation sous prétexte de « mode debug ».

## D. Sauvegardes, transaction et récupération
- Avant de fermer le logiciel, stopper de nouvelles opérations métier et attendre les transactions en cours; sinon proposer « Réessayer plus tard ».
- Créer **une sauvegarde SQLite cohérente vérifiée** avec `db.pre_update_backup()` ou équivalent et inscrire son chemin; ne jamais supprimer la base ou nettoyer les sauvegardes durant l'upgrade.
- Vérifier l'intégrité de la base, disposer d'espace disque suffisant pour archive temporaire + backup + extraction, isoler et nettoyer les fichiers temporaires selon une stratégie déterministe.
- Un superviseur/lanceur indépendant (sous les **mêmes droits utilisateur**, pas un service privilégié) contrôle fermeture des processus, mutex, lancement de package déjà vérifié, retour du processus, version installée et relance de l'interface.
- En cas d'erreur : garder la base et la sauvegarde, journaliser l'étape/exit code sans secret, expliquer reprise et maintenance, **ne pas affirmer une restauration automatique** tant qu'elle n'est pas prouvée, et refuser de migrer rétroactivement une DB incompatible.
- Réouverture validée par version + `PRAGMA integrity_check`/test de compatibilité; en cas d'échec, statut de récupération manuel documenté avec paquet précédent vérifié. Pas de rollback MSI automatique improvisé contre la politique anti-downgrade.

## E. Compatibilité, publication et sécurité
- La **première** version capable de vérifier Ed25519 doit être installée de manière approuvée par SEL; l'actuelle `0.3.24` refuse les paquets sans Authenticode. On ne peut pas déployer cette transition en se contentant de remplacer le manifeste distant.
- Préserver l'API de facturation, `%LOCALAPPDATA%\SEL Facturation\sel.sqlite3`, les instantanés de factures, la numérotation, les paiements, `session.key`, le schéma export JSON `sel-facturation-portable v1`, l'utilisation hors connexion.
- La mise à jour doit rester compatible avec les évolutions présentes dans PR #41; travailler sur branche séparée et minimiser les conflits.
- Mettre à jour les tests de l'ancien code qui attendent un rejet `signed:false` **uniquement après** les avoir remplacés par tests cryptographiques couvrant rejets et chemin unsigned authentifié.
- Ne pas exécuter du shell arbitraire à partir des URL, arguments, noms de fichiers, versions, notes ou réponses JSON; contrôler les chemins, les répertoires temporaires, symlinks/reparse points et les droits d'accès; protéger le pont JS natif.
- Ne pas permettre les changements silencieux de serveur de mise à jour, de clé publique, de canal ou de policy depuis un manifeste non authentifié.

## F. Références techniques vérifiables
- Microsoft : https://learn.microsoft.com/en-us/windows/win32/msi/using-windows-installer-with-uac
- Microsoft : https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation
- Microsoft : https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution
- Toute dérogation à une obligation MUST est un blocage et doit figurer dans la PR avant validation.
