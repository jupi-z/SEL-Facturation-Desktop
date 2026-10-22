# Exploitation propriétaire — clés, publication et première installation

Ce guide concerne la **signature du manifeste SafeUpdate par SEL**, pas une signature Microsoft. Il ne doit contenir aucune clé privée réelle, mot de passe, empreinte de package fictive ni adresse e-mail personnelle.

## Décisions retenues
- Application et setup **non signés Authenticode** jusqu'à nouvel ordre.
- Paquets **authentifiés en interne** uniquement par manifest Ed25519 signé + SHA-256/size exact.
- Le binaire client de première mise à jour embarque la **clé publique**; propriétaire conserve hors dépôt la **clé privée**.
- Pas de privilège admin permanent, pas de contournement UAC ni Windows SmartScreen.
- Processus update **explicitement accepté par utilisateur**; facturation offline inchangée.

## Mise en place unique
1. Préparer un poste de maintenance de confiance; générer une paire Ed25519 avec un script auditable produit par Codex, vérifier que les fichiers privés sont créés hors du clone Git. Pas de hardcode dans `update_policy.py` d'une clé de test.
2. Chiffrer la clé privée PKCS8 avec une phrase secrète robuste, stocker son original et une sauvegarde chiffrée **hors GitHub/CI**, limiter les permissions système; noter responsable/lieu de conservation hors du dépôt.
3. Insérer UNIQUEMENT la clé publique + key-id dans l'updater, vérifier son fingerprint via une seconde voie et compiler la première version avec CI Windows + recette.
4. Installer **une fois** cette première version SafeUpdate sous contrôle SEL avec une source de distribution explicitement approuvée. Une version déjà en circulation v0.3.24 n'a pas la clé et n'est pas auto-migrable par ce seul protocole.
5. Pour chaque version suivante, faire builder EXE/MSI/bootstrapper par un pipeline vérifiable sans clé privée; contrôler le résultat avant de publier.
6. Signer hors ligne **les octets exacts** de `updates/latest.json` construit à partir des artefacts réels; produire `latest.sig.json`.
7. Re-vérifier avec la clé publique en script indépendant; faire approuver la release; publier assets puis paire (manifeste, signature) atomiquement.
8. Effectuer une mise à jour canari sur un PC de test Windows utilisateur standard avant exposition à l'ensemble des agents SEL.
9. Archiver release connue saine, manifeste+signature, SHA256, logs de build anonymisés, date d'approbation et résultat de recette. En cas de doute, **désactiver publication du canal**, pas la sécurité du client.

## Plan d'incident
- **Clé privée perdue :** stop canal, utiliser sauvegarde privée offline; sinon nouvelle bootstrap de confiance manuelle.
- **Clé compromise :** stop publication immédiat, enquête sur releases, clé remplacée via update de transition si ancienne clé encore sûre, sinon installation manuelle d'un nouveau root de confiance.
- **Serveur GitHub non disponible :** le logiciel continue à fonctionner; aucune facture bloquée.
- **Package valide bloqué par OS :** suivre administration/politiques Windows officielles; aucun contournement. Un poste géré peut exiger l'IT de SEL.
- **Crash après update :** conserver backup et journaux, vérifier DB, procédure de réparation signée au sens *manifeste*; ne pas restaurer de manière destructrice les instantanés de factures.
- **Échec de signature de manifeste :** ne pas publier, ne jamais définir `allow_unsafe_updates=true` ni recourir à un simple hash hébergé dans le même dépôt sans signature.

## Livrables pour le propriétaire
Codex doit fournir : instructions d'installation bootstrap, génération clés (outil et documentation, **sans exécuter la génération de prod à la place du propriétaire**), signatures de test, release dry-run, test Windows réel ou limites, procédure de rotation et recovery, liste exacte des réglages restant à configurer.

### Note sur les avertissements Windows
Une signature Ed25519 sur les métadonnées ne confère aucune réputation Microsoft. Certains postes Windows pourront afficher un avertissement ou bloquer un EXE non signé. Toute promesse « aucune intervention Windows, sur 100 % des PC » serait techniquement infondée; notre objectif réaliste est « un clic dans SEL, installation autonome lorsque la politique du PC le permet ».
