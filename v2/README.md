# SEL V2 — application Windows connectée et backend

L’application Windows SEL V2 est un client du service central. Toute ouverture
et toute opération métier nécessitent une connexion au serveur SEL et une
session utilisateur valide. Elle ne fournit aucun mode métier hors connexion.

## Développement du service central

```text
cargo run -p sel-api
```

Le service central écoute sur `127.0.0.1:8080` par défaut. `SEL_API_BIND` et
`SEL_API_PORT` sont configurables. Si `DATABASE_URL` est fourni, le démarrage
ouvre PostgreSQL, applique les migrations versionnées et utilise les repositories
transactionnels centraux. Le binaire central active une garde d'authentification
stricte : `SEL_API_AUTH_TOKEN` doit être injecté hors Git et chaque appel `/v1`
doit présenter ce jeton. Les tests en mémoire utilisent uniquement le garde
historique de développement et ne doivent pas être exposés.

Le client Windows V2 exige une
session utilisateur auprès de l'API centrale, lit et écrit les données métier
directement dans PostgreSQL via cette API et verrouille ses écrans métier quand
le service est indisponible. Il ne démarre pas `sel-local` et ne propose pas
d'opérations métier hors connexion. Si le serveur répond mais que ses
référentiels ne sont pas encore configurés, l'administrateur peut se connecter
pour les initialiser; les parcours commerciaux restent verrouillés. Les jetons de renouvellement sont protégés
par DPAPI sur Windows; la session peut survivre aux redémarrages, avec une
expiration absolue après trois jours. Les PDF sont composés sur le poste depuis
les données structurées renvoyées par l'API; aucun fichier de facture n'est
transmis au serveur.

Les nouvelles factures affichent une référence `SEL-AAAAMMJJ-HHMMSS` selon le
fuseau de Lubumbashi (UTC+2). L’identifiant unique reste dans le QR interne;
les factures historiques gardent leur référence d’origine. Les reçus et ventes
solaires conservent un suffixe unique dans leur référence.

Les anciennes sources de l’agent local et de synchronisation sont conservées
dans le dépôt pour préserver l’historique technique. Elles ne sont plus
compilées, embarquées, exécutées ni utilisées par les workflows de distribution
Windows. Les lanceurs Python V1/V2 et leurs scripts de build locaux sont
neutralisés afin qu’ils ne puissent pas redémarrer ou redistribuer l’ancien
mode par erreur. Les données métier sont exclusivement lues et écrites sur le serveur.
Les jetons de session protégés par DPAPI et les clés d’idempotence conservées
par le client sont des éléments techniques de session et de reprise de requête;
ils ne constituent ni une base métier locale ni un mode hors connexion.

## Règles de sécurité

- Les montants sont des `Decimal`, jamais des flottants.
- Une prévisualisation utility produit toujours deux lignes : électricité et eau.
- `0 m³` est visible et ne reçoit aucun tarif inventé ; un volume positif exige
  un tarif/taxe configuré.
- Les documents sont renvoyés en JSON structuré. Aucun endpoint PDF et aucun
  champ de fichier de facture ne sont prévus côté serveur.
- Les ressources LiveUpdate sont déclaratives, vérifiées Ed25519 + SHA-256,
  soumises à expiration, anti-rollback, contrôle de chemin et retour arrière.
- Le binaire de production refuse tout bearer arbitraire lorsque
  `SEL_API_AUTH_TOKEN` est absent ou différent ; ce jeton de déploiement est une
  barrière minimale, pas encore un remplacement des sessions utilisateur, du
  RBAC fin et de la rotation de jetons prévus par le cahier des charges.

## Tests

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Les migrations PostgreSQL et SQLite sont sous `migrations/`. Le compose de test
est `deploy/compose.test.yml`; il n'est pas une configuration de production.
