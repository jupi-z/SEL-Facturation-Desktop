# SEL V2 Backend

Backend Rust autonome et sans interface graphique pour la prochaine architecture SEL.

## Exécution locale

```text
cargo run -p sel-api
cargo run -p sel-local
```

Le service central écoute sur `127.0.0.1:8080` par défaut. `SEL_API_BIND` et
`SEL_API_PORT` sont configurables. Si `DATABASE_URL` est fourni, le démarrage
ouvre PostgreSQL, applique les migrations versionnées et utilise les repositories
transactionnels centraux. Le binaire central active une garde d'authentification
stricte : `SEL_API_AUTH_TOKEN` doit être injecté hors Git et chaque appel `/v1`
doit présenter ce jeton. Les tests en mémoire utilisent uniquement le garde
historique de développement et ne doivent pas être exposés.

Le service local reste limité à `127.0.0.1:8090` et expose l'état offline,
le déclenchement de synchronisation et l'état des ressources de confiance.

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
