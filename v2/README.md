# SEL V2 Backend

Backend Rust autonome et sans interface graphique pour la prochaine architecture SEL.

## Exécution locale

```text
cargo run -p sel-api
cargo run -p sel-local
```

Le service central écoute sur `127.0.0.1:8080` par défaut. `SEL_API_BIND` et
`SEL_API_PORT` sont configurables. Si `DATABASE_URL` est fourni, le démarrage
ouvre PostgreSQL et applique uniquement les migrations versionnées ; la
persistance métier de l'API n'est pas encore activée dans ce prototype de
contrats et doit rester hors production.

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
- Le bearer accepté par `sel-api` est un garde de développement uniquement ;
  l'authentification utilisateur, le RBAC, les grants offline signés et le
  stockage PostgreSQL transactionnel doivent être validés avant toute mise en
  production.

## Tests

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Les migrations PostgreSQL et SQLite sont sous `migrations/`. Le compose de test
est `deploy/compose.test.yml`; il n'est pas une configuration de production.
