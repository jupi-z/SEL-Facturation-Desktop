# SEL V2 — paquet serveur contrôlé

Ce dossier décrit le déploiement central; le dépôt ne contient aucun secret, aucune clé Ed25519 privée ni copie de données SEL réelles. L’instance de production utilise PostgreSQL et contient des données opérationnelles. Le fichier `compose.test.yml` reste réservé aux tests ; `compose.production.example.yml` est un modèle à copier dans un environnement protégé.

## Hôtes publics configurés

- API SEL V2 : `https://sel.shuleops.com`
- Distribution statique SafeUpdate : `https://update.sel.shuleops.com`

Les deux noms DNS doivent pointer vers le serveur public SEL et les ports TCP 80/443 doivent parvenir au service Caddy. Le Caddyfile utilise HTTPS automatique ACME pour chacun des hôtes; seul Caddy est connecté au réseau externe `sel-v2-edge`. PostgreSQL et `sel-api` restent sur le réseau Docker interne. L’API n’est pas exposée directement au réseau Internet. Si un proxy ou service Hostinger occupe déjà les ports 80/443, ne pas démarrer cette publication de ports en conflit : configurer le proxy existant pour router ces deux hôtes vers Caddy et documenter cette terminaison TLS.

Le canal de mise à jour sert uniquement les chemins `/updates/*`, `/releases/*` et `/resources/*`, en lecture seule. Les manifestes `latest.json` et `latest.sig.json` ne sont pas mis en cache. Les fichiers de release doivent être immuables; les clés privées de signature ne doivent jamais être placées dans `v2/deploy/updates/`. Ce répertoire est initialement vide : l’hôte d’updates répondra 404 tant qu’un manifeste signé et ses ressources n’auront pas été publiés. Le client V2 ne possède pas encore de récupérateur HTTP LiveUpdate complet; cette configuration prépare l’hébergement, elle n’active pas seule les mises à jour.

Chaque poste enrôlé doit recevoir dans son `device.json` une URL centrale correspondant à `https://sel.shuleops.com`, avec ses jetons d’appareil et de synchronisation provisionnés séparément par la procédure sécurisée d’enrôlement. Ne réutiliser aucun jeton de test.

Valeur centrale à inscrire dans le `device.json` créé par l’enrôlement (les secrets et identifiants restent spécifiques au poste et à l’environnement) :

```json
{
  "central_api_url": "https://sel.shuleops.com"
}
```

Le client V1 SafeUpdate utilise par défaut `https://update.sel.shuleops.com/updates/latest.json`; l’override `SEL_UPDATE_MANIFEST_URL` reste disponible pour les environnements de test. Le client V2 ne télécharge pas encore LiveUpdate automatiquement; ne pas afficher ce canal comme opérationnel et ne pas publier de manifeste jusqu’à ce que son transport, la clé publique épinglée et la procédure de signature/rollback soient intégrés et validés.

## Pré-requis bloquants

- serveur Linux administré par SEL, avec Docker/Compose, ports 80/443 accessibles et contrôle des deux enregistrements DNS ;
- émission ACME réussie par Caddy pour `sel.shuleops.com` et `update.sel.shuleops.com` ;
- identifiant d’organisation stable ;
- paire de clés Ed25519 de production pour les autorisations hors ligne, avec la clé privée uniquement sur le serveur et la clé publique correspondante dans l’API ;
- mots de passe PostgreSQL générés hors dépôt ;
- sauvegarde PostgreSQL testée sur une base non productive ;
- procédure de retour arrière approuvée.

Le serveur doit être le seul point accessible depuis les postes. PostgreSQL reste sur le réseau privé Compose et aucun PDF, image ou archive de facture n’est stocké par l’API.

## Préparation

```bash
cp v2/deploy/compose.production.example.yml /srv/sel-v2/compose.yml
cp v2/deploy/Caddyfile.example /srv/sel-v2/Caddyfile
mkdir -m 700 /srv/sel-v2/updates
chmod 600 /srv/sel-v2/.env
docker compose -f /srv/sel-v2/compose.yml --env-file /srv/sel-v2/.env config
```

Le fichier `.env` doit contenir uniquement des valeurs injectées par l’administrateur : `POSTGRES_DB`, `POSTGRES_ADMIN_USER`, `POSTGRES_ADMIN_PASSWORD`, `POSTGRES_APP_USER`, `POSTGRES_APP_PASSWORD`, `SEL_OFFLINE_GRANT_PUBLIC_KEY`, `SEL_OFFLINE_GRANT_SIGNING_KEY`, `SEL_ORGANIZATION_ID` et `SEL_ORGANIZATION_NAME`. Protéger `.env` par `chmod 600`; ne jamais copier la clé privée dans Git, un installateur ou les logs. Au démarrage commercial, l’API vérifie que les deux clés Ed25519 correspondent et refuse le démarrage en cas de discordance.

Le premier démarrage de l’application Windows requiert une connexion administrateur centrale. La commande « Activer ce poste » enrôle l’appareil et émet atomiquement une autorisation hors ligne signée, limitée à 7 jours. Le jeton de synchronisation propre au poste n’est stocké que sous forme de hash SHA-256 dans PostgreSQL et peut être invalidé par révocation de l’appareil. SEL a confirmé qu’aucun plafond de montant hors ligne n’est appliqué pour l’unique poste initial; les rôles et les portées de commandes restent contrôlés côté serveur.

## Déploiement contrôlé

Les sources destinées à une migration SQLx doivent être récupérées sur Linux à partir du commit immuable à déployer (`git checkout --detach <commit>`, `core.autocrlf=false`). Ne construisez pas le contexte Docker depuis une archive Git créée sous Windows : la conversion CRLF peut changer les checksums intégrés aux migrations, même si le SQL paraît identique.

Avant de remplacer l’API :

1. Identifiez et conservez le digest de l’image actuellement en service.
2. Créez un dump PostgreSQL daté et privé, puis vérifiez qu’il est listable avec `pg_restore -l`; consignez son SHA-256.
3. Comparez les SHA-384 des migrations déjà appliquées avec les checksums de `_sqlx_migrations` sur la base cible. Tout écart doit bloquer le déploiement avant l’arrêt de l’API.
4. Construisez le candidat depuis le checkout Linux exact, puis démarrez uniquement `sel-api`.
5. Vérifiez `/health/ready`, l’état Docker, le nombre/version de migrations et les données de catalogue attendues.

Les migrations s’exécutent au démarrage via SQLx; toute erreur arrête l’API. Préparez l’image de retour arrière avant la bascule. Aucune migration destructive n’est autorisée par cette procédure.

Après publication d’un manifeste signé, contrôler aussi `curl --head --fail https://update.sel.shuleops.com/updates/latest.json`. Avant cette publication, le `404` du canal d’updates est attendu. La release Windows manuelle ne rend pas SafeUpdate natif opérationnel. Vérifier les certificats des deux hôtes avant d’installer/configurer un client.

## Sauvegarde et retour arrière

Utiliser `sel-cli backup-postgres` dans un conteneur temporaire avec `DATABASE_URL` injectée, conserver le dump chiffré hors serveur applicatif et tester la restauration sur une base isolée avec `sel-cli restore-postgres`. Ne jamais restaurer sur la base réelle sans fenêtre de maintenance, sauvegarde préalable et validation SEL.

Le retour arrière consiste à redéployer l’image précédente identifiée par son digest, sans supprimer le volume PostgreSQL. Pour le canal updates, ne remplacer le manifeste publié qu’après validation de la signature, de la séquence, des empreintes et de la compatibilité; conserver une copie de la dernière paire manifeste/signature connue bonne hors du serveur. Si une migration non rétrocompatible est introduite, le déploiement est bloqué jusqu’à une procédure de migration/rollback dédiée.

## État de production

Les observations DNS/HTTPS ci-dessus dans l’historique du projet décrivent le contrôle initial et ne sont plus l’état courant. Au 6 novembre 2026, `https://sel.shuleops.com/health/ready` retourne `configured:true`; l’API et PostgreSQL sont sains. Le commit déployé est `c8f1fc47d113314a2408d342e06e8b57d9d776cf`, 21 migrations sont appliquées et le déploiement de la ligne de communication est détaillé dans `Docs/SEL_V2_BACKEND/IMPLEMENTATION_REPORT.md`. Le canal SafeUpdate reste non publié : `https://update.sel.shuleops.com/updates/latest.json` retourne 404 tant qu’un manifeste de production correctement signé n’a pas été provisionné.
