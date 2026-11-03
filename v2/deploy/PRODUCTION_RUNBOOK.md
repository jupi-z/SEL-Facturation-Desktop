# SEL V2 — paquet serveur contrôlé

Ce dossier prépare un déploiement central, mais ne contient aucun secret, aucune clé Ed25519 privée et aucune donnée SEL réelle. Le fichier `compose.test.yml` reste réservé aux tests ; `compose.production.example.yml` est un modèle à copier dans un environnement protégé.

## Pré-requis bloquants

- serveur Linux administré par SEL, avec Docker/Compose et un nom DNS ;
- certificats TLS valides placés hors Git dans `certs/` ;
- identifiant d’organisation stable ;
- clé publique Ed25519 de production pour les autorisations hors ligne ;
- mots de passe PostgreSQL générés hors dépôt ;
- sauvegarde PostgreSQL testée sur une base non productive ;
- procédure de retour arrière approuvée.

Le serveur doit être le seul point accessible depuis les postes. PostgreSQL reste sur le réseau privé Compose et aucun PDF, image ou archive de facture n’est stocké par l’API.

## Préparation

```bash
cp v2/deploy/compose.production.example.yml /srv/sel-v2/compose.yml
cp v2/deploy/Caddyfile.example /srv/sel-v2/Caddyfile
mkdir -m 700 /srv/sel-v2/certs
chmod 600 /srv/sel-v2/.env
docker compose -f /srv/sel-v2/compose.yml --env-file /srv/sel-v2/.env config
```

Le fichier `.env` doit contenir uniquement des valeurs injectées par l’administrateur : `POSTGRES_DB`, `POSTGRES_ADMIN_USER`, `POSTGRES_ADMIN_PASSWORD`, `POSTGRES_APP_USER`, `POSTGRES_APP_PASSWORD`, `SEL_OFFLINE_GRANT_PUBLIC_KEY`, `SEL_ORGANIZATION_ID` et `SEL_ORGANIZATION_NAME`.

## Démarrage contrôlé

```bash
docker compose -f /srv/sel-v2/compose.yml --env-file /srv/sel-v2/.env build --pull
docker compose -f /srv/sel-v2/compose.yml --env-file /srv/sel-v2/.env up -d
curl --fail --silent https://sel.example.invalid/health/ready
```

Les migrations sont exécutées par l’API au démarrage via SQLx. Toute erreur arrête le conteneur ; aucune migration destructive n’est autorisée par cette procédure.

## Sauvegarde et retour arrière

Utiliser `sel-cli backup-postgres` dans un conteneur temporaire avec `DATABASE_URL` injectée, conserver le dump chiffré hors serveur applicatif et tester la restauration sur une base isolée avec `sel-cli restore-postgres`. Ne jamais restaurer sur la base réelle sans fenêtre de maintenance, sauvegarde préalable et validation SEL.

Le retour arrière consiste à redéployer l’image précédente identifiée par son digest, sans supprimer le volume PostgreSQL. Si une migration non rétrocompatible est introduite, le déploiement est bloqué jusqu’à une procédure de migration/rollback dédiée.

## Statut de ce dépôt

Ce paquet est prêt pour revue technique et déploiement contrôlé, mais aucun serveur réel, certificat TLS, secret PostgreSQL, clé de production ou restauration sur infrastructure SEL n’a été vérifié dans cette session.
