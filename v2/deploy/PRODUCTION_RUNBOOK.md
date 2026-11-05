# SEL V2 — paquet serveur contrôlé

Ce dossier prépare un déploiement central, mais ne contient aucun secret, aucune clé Ed25519 privée et aucune donnée SEL réelle. Le fichier `compose.test.yml` reste réservé aux tests ; `compose.production.example.yml` est un modèle à copier dans un environnement protégé.

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
- clé publique Ed25519 de production pour les autorisations hors ligne ;
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

Le fichier `.env` doit contenir uniquement des valeurs injectées par l’administrateur : `POSTGRES_DB`, `POSTGRES_ADMIN_USER`, `POSTGRES_ADMIN_PASSWORD`, `POSTGRES_APP_USER`, `POSTGRES_APP_PASSWORD`, `SEL_OFFLINE_GRANT_PUBLIC_KEY`, `SEL_ORGANIZATION_ID` et `SEL_ORGANIZATION_NAME`.

## Démarrage contrôlé

```bash
docker compose -f /srv/sel-v2/compose.yml --env-file /srv/sel-v2/.env build --pull
docker compose -f /srv/sel-v2/compose.yml --env-file /srv/sel-v2/.env up -d
curl --fail --silent https://sel.shuleops.com/health/ready
```

Après publication d’un manifeste signé, contrôler aussi `curl --head --fail https://update.sel.shuleops.com/updates/latest.json`. Avant cette publication, le `404` du canal d’updates est attendu. Vérifier les certificats des deux hôtes avant d’installer/configurer un client. Les migrations sont exécutées par l’API au démarrage via SQLx. Toute erreur arrête le conteneur ; aucune migration destructive n’est autorisée par cette procédure.

## Sauvegarde et retour arrière

Utiliser `sel-cli backup-postgres` dans un conteneur temporaire avec `DATABASE_URL` injectée, conserver le dump chiffré hors serveur applicatif et tester la restauration sur une base isolée avec `sel-cli restore-postgres`. Ne jamais restaurer sur la base réelle sans fenêtre de maintenance, sauvegarde préalable et validation SEL.

Le retour arrière consiste à redéployer l’image précédente identifiée par son digest, sans supprimer le volume PostgreSQL. Pour le canal updates, ne remplacer le manifeste publié qu’après validation de la signature, de la séquence, des empreintes et de la compatibilité; conserver une copie de la dernière paire manifeste/signature connue bonne hors du serveur. Si une migration non rétrocompatible est introduite, le déploiement est bloqué jusqu’à une procédure de migration/rollback dédiée.

## Statut de ce dépôt

Les enregistrements DNS des deux hôtes ont été résolus depuis l’environnement de développement vers `187.77.175.51`. Au contrôle HTTPS, `sel.shuleops.com/health/ready` retournait `404` et le certificat présenté par `update.sel.shuleops.com` ne correspondait pas au nom demandé. Ces réponses montrent que les noms ne desservent pas encore le service SEL attendu avec des certificats corrects. Le Caddyfile et Compose sont configurés, mais aucun déploiement serveur, renouvellement TLS, secret PostgreSQL, clé de production, publication de manifeste ou restauration sur infrastructure SEL n’a été effectué. La mise en service reste bloquée jusqu’à vérification de l’hébergement/proxy sur l’adresse résolue et de l’émission des certificats.
