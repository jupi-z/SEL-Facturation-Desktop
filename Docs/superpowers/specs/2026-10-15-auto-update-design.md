# SEL Facturation — Conception des mises à jour automatiques Windows

## Objectif

Permettre à SEL Facturation Desktop de détecter, télécharger et installer une mise à jour Windows de manière contrôlée, hors ligne par défaut, sans perdre les données locales et sans exécuter un binaire non vérifié.

La fonctionnalité concerne Windows 10/11 et conserve la pile Python/Flask/SQLite, le format portable `sel-facturation-portable` v1 et le bootstrapper WiX officiel.

## Périmètre retenu

- Une source HTTPS configurable par défaut sur les GitHub Releases du dépôt officiel.
- Un manifeste JSON signé ou, tant que la signature de code n'est pas disponible, vérifié au minimum par HTTPS et SHA-256 avec état explicitement non final.
- Une action manuelle « Rechercher une mise à jour » et une vérification discrète au démarrage, avec délai pour éviter de ralentir l'ouverture.
- Téléchargement dans un répertoire temporaire hors du dossier d'installation.
- Vérification de version : uniquement une version strictement supérieure à la version courante ; aucun downgrade.
- Vérification de taille, SHA-256 et extension attendue (`.exe`).
- Sauvegarde SQLite cohérente avant lancement de l'installation.
- Lancement du bootstrapper WiX avec élévation Windows ; l'application ne remplace jamais son propre EXE pendant qu'elle tourne.
- Conservation de `%LOCALAPPDATA%\SEL Facturation` pendant l'upgrade.
- Reprise propre si le réseau est indisponible, si le manifeste est invalide, si le téléchargement échoue ou si l'utilisateur refuse l'élévation.
- Journal local sans mot de passe, clé de session ni contenu sensible.

## Flux utilisateur

1. L'application démarre normalement même sans réseau.
2. Le vérificateur interroge le manifeste avec un timeout court et sans bloquer l'interface.
3. Si aucune version supérieure n'est disponible, aucune interruption n'est affichée.
4. Si une version est disponible, l'utilisateur voit la version, la taille, la date et l'état de vérification.
5. Après confirmation, le fichier est téléchargé puis vérifié.
6. Une sauvegarde SQLite est créée et contrôlée avant lancement.
7. L'application demande à l'utilisateur de fermer les travaux en cours, se ferme proprement et lance le bootstrapper.
8. Après redémarrage, l'application affiche le résultat ou l'échec récupérable.

## Sécurité

- TLS obligatoire ; aucune URL HTTP ni redirection non HTTPS acceptée.
- Le manifeste ne peut pas imposer une version inférieure, un chemin arbitraire ou une commande arbitraire.
- Le téléchargement est écrit dans un fichier temporaire puis renommé après vérification.
- Le hash attendu doit correspondre exactement au contenu téléchargé.
- La signature Authenticode du bootstrapper sera exigée pour le statut de production ; sans certificat, la fonction reste marquée pilote et ne supprime pas SmartScreen.
- Aucun téléchargement ni exécution automatique silencieuse sans consentement utilisateur et élévation Windows.

## Compatibilité et reprise

- Les données restent dans `%LOCALAPPDATA%\SEL Facturation` et ne sont jamais supprimées par l'updater.
- La sauvegarde pré-upgrade utilise le mécanisme SQLite existant et porte un nom horodaté.
- Une mise à jour interrompue laisse l'ancienne version ou le bootstrapper Windows responsable du rollback ; l'application ne modifie pas le MSI elle-même.
- La version courante et le dernier résultat de mise à jour sont conservés uniquement dans des fichiers de journalisation non sensibles.
- Le canal et l'URL doivent pouvoir être remplacés plus tard par un serveur SEL ou Monarque Connect sans changer le format des données.

## Tests d'acceptation

- Manifeste valide avec version supérieure : proposition affichée.
- Version identique ou inférieure : aucune installation proposée.
- Hash incorrect, URL HTTP, extension inattendue, taille excessive ou JSON invalide : téléchargement refusé.
- Timeout, absence de réseau ou erreur serveur : application utilisable et erreur non bloquante.
- Sauvegarde SQLite valide avant lancement.
- Aucun fichier de données supprimé ou modifié par le téléchargement.
- Le bootstrapper est lancé uniquement après confirmation et vérification.
- Test Windows réel avec une version précédente et vérification de la base, des snapshots, des sauvegardes et de l'unicité de l'installation.
- Le résultat final reste `pilote non validé sur poste cible SEL` tant que la signature et l'essai sur le poste cible ne sont pas réalisés.

## Hors périmètre de cette première livraison

- Service Windows permanent.
- Mise à jour forcée ou silencieuse.
- Migration de schéma destructive.
- Remplacement du bootstrapper WiX par un nouveau framework.
- Suppression de l'avertissement SmartScreen sans certificat de signature.
