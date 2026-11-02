# SEL V2 Backend — rapport d'implémentation

Date de contrôle : 2 novembre 2026, Africa/Lubumbashi  
Branche : `monarque/sel-v2-backend`  
Commits de référence : socle précédent `40a97fc`, implémentation `402fece`, rapport/documentation `88fc431288fac3ce1491965a418c096adb7c5986`, correctifs de cette reprise `ddd78bb`, `e7b2294`, `70afddd`, `a5faffa`.
Périmètre : backend Rust/API, migrations, synchronisation et LiveUpdate. Aucun nouveau frontend, aucune fusion dans `main`, aucune release et aucun déploiement réel.

## Résumé exécutif

Le dépôt contient maintenant un workspace Rust autonome, les contrats JSON versionnés, un moteur de calcul Decimal pour l'électricité et l'eau, des migrations PostgreSQL/SQLite, des repositories SQLx transactionnels, des routes commerciales centrales persistantes, un journal/outbox SQLite durable, un protocole de synchronisation central idempotent, un agent local loopback, un vérificateur LiveUpdate Ed25519/SHA-256 et une commande de simulation d'import V1.

### Actualisation de la reprise du 2 novembre 2026

- `ddd78bb` agrège les affectations d'un même paiement avant validation, verrouille les créances dans un ordre déterministe et couvre la concurrence PostgreSQL ainsi que la double affectation de 7 + 7 USD sur une créance de 10 USD.
- `e7b2294` impose les identités organisation/appareil en mode commercial, refuse les jetons d'authentification absents ou trop courts et empêche les changements d'identité d'une base SQLite existante.
- `70afddd` ajoute les opérations locales durables de création client, facture eau + électricité, paiement partiel et consultation des soldes, avec journal et outbox dans la même transaction.
- `a5faffa` ajoute le worker push/pull SQLite, le curseur central durable, les accusés persistants, la projection des clients reçus, la reprise des événements `CUSTOMER_UPDATED`, `PAYMENT_RECORDED` et `UTILITY_INVOICE_ISSUED`, ainsi que l'identité d'agent/grant hors ligne persistée.
- Les tests réellement rejoués sur ce jalon sont : `cargo test -p sel-local` (4 tests), `cargo test -p sel-storage` (6 tests non ignorés), `cargo test -p sel-storage --test postgres_sync_financial -- --ignored` (2 tests PostgreSQL réels) et `cargo test -p sel-storage --test sqlite_identity` (1 test ciblé). La campagne globale fmt/Clippy/workspace doit encore être relancée sur `a5faffa` avant de fermer ce jalon.

Les preuves locales exécutées sur ce jalon sont vertes : `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (15 tests non ignorés, les scénarios PostgreSQL étant explicitement ignorés dans cette commande), puis `cargo test --workspace -- --ignored` contre PostgreSQL 16 réel (5 tests réussis). Les scénarios couvrent la reprise après redémarrage, la facture utility à deux lignes avec eau à zéro, le paiement partiel idempotent, la vente solaire avec une créance et un échéancier exacts, et la synchronisation centrale avec conflit de payload durable.

Le verdict reste NO-GO pour la production commerciale. Les parcours centraux testés ne reposent plus sur les HashMaps quand `DATABASE_URL` est fourni, mais l'authentification de production est encore une garde par jeton configuré (pas encore sessions courtes, rotation et RBAC fin), l'agent local ne rejoue pas encore toutes les opérations métier dans SQLite, la synchronisation bidirectionnelle complète et les grants offline signés restent à finaliser, et l'import V1 est actuellement un dry-run vérifié sans écriture.

## Corrections et livrables

- `v2/crates/sel-domain` : montants `rust_decimal::Decimal`, arrondi à deux décimales, règles de non-négativité, facture utility toujours composée de `ELECTRICITY` et `WATER`, eau à `0 m³` visible sans tarif inventé, eau positive bloquée sans tarif/taxe configurés, échéancier solaire dont la somme doit égaler exactement le principal.
- `v2/crates/sel-contracts` : DTO API, version de protocole/document, erreurs Problem Details, document-data JSON, paiements, sync et contrats solaires.
- `v2/crates/sel-storage` : ouverture/migrations PostgreSQL et SQLite via SQLx ; PostgreSQL persiste clients, factures, lignes, créances, paiements, ventes solaires, contrats et résultats de sync dans des transactions ; SQLite active WAL et conserve journal de commandes, outbox, inbox et conflits sans supprimer le journal.
- `v2/migrations/postgres/0001_init.sql` et `v2/migrations/sqlite/0001_init.sql` : modèle structuré clients, services, produits, tarifs, taxes, FX, ventes, factures, lignes, créances, plans, paiements, audit, idempotence et sync. Aucun champ/stockage de fichier de facture.
- `v2/crates/sel-api` : health, capacités, clients, `If-Match`/ETag de modification, preview/issue utility, paiements et soldes de créances, document-data JSON, ventes solaires et activation d'échéancier, sync push/pull/status, métadonnées de configuration/ressources. En mode central, les routes commerciales utilisent PostgreSQL ; les écritures financières exigent `Idempotency-Key` et rejouent la réponse durable.
- `v2/crates/sel-sync` : déduplication `(device_id,event_id)`, contrôle SHA-256 du payload, résultats `ACCEPTED`/`DUPLICATE`/`CONFLICT`, curseur central, manifeste LiveUpdate signé Ed25519, expiration, séquence anti-rollback, chemins relatifs, taille/hash des ressources, activation et rollback connu-bon.
- `v2/crates/sel-local` : endpoints loopback `/local/v1/sync/status`, `/sync/trigger` et `/updates/status` branchés sur SQLite WAL, sans proxy aveugle ni affirmation de synchronisation centralisée ; les commandes locales sont atomiquement journalisées et mises en outbox.
- `v2/crates/sel-cli` : `migrate-v1 --dry-run <sel-facturation-portable-v1.json>` vérifie le format/schema, calcule le SHA-256, compte les entités, détecte les références de paiements manquantes et garantit zéro écriture.
- `v2/openapi/sel-v2.yaml`, `v2/deploy/compose.test.yml`, `v2/deploy/Dockerfile.test`, `.github/workflows/sel-v2-backend.yml`, `v2/README.md` et `.dockerignore`.

## Résultats de tests exécutés

Environnement Rust : conteneur `rust:1.85-bookworm`, toolchain compatible avec la CI.  
Commandes :

```text
docker run --rm -v "${PWD}\v2:/workspace" -w /workspace rust:1.85-bookworm cargo fmt --all -- --check
docker run --rm -v "${PWD}\v2:/workspace" -w /workspace rust:1.85-bookworm cargo clippy --workspace --all-targets -- -D warnings
docker run --rm -v "${PWD}\v2:/workspace" -w /workspace rust:1.85-bookworm cargo test --workspace
python -m pytest -q
docker compose -f v2/deploy/compose.test.yml config
docker compose -f v2/deploy/compose.test.yml up -d --build
Invoke-RestMethod http://127.0.0.1:18080/health/live
docker exec deploy-postgres-1 psql -U sel_test -d sel_v2_test -Atc "SELECT count(*) FROM information_schema.tables WHERE table_schema='public'"
docker compose -f v2/deploy/compose.test.yml down -v
docker exec -e TEST_DATABASE_URL=postgres://sel:postgres@host.docker.internal:15432/sel_v2_test sel-v2-rust-dev cargo test --workspace -- --ignored
```

Résultats :

- Rust : PASS, formatage vérifié ; PASS, Clippy sans warning ; PASS, `cargo test --workspace` après correction du chemin du test OpenAPI ; PASS, campagne séparée des cinq tests PostgreSQL ignorés.
- PostgreSQL : PASS, tests réels `postgres_persistence`, `postgres_customer_api`, `postgres_commercial`, `postgres_solar` et `postgres_sync` ; les réponses d'idempotence sont relues depuis `idempotency_keys`, les compteurs SQL restent à une facture/deux lignes/une créance et un seul plan solaire, et la sync vérifie inbox/change/conflict après commit.
- API : PASS, facture utility JSON à deux lignes avec eau zéro ; PASS, tarifs approuvés/inactifs lus côté central ; PASS, modification client protégée par révision ; PASS, paiement partiel et solde ; PASS, vente solaire et activation 400+600 ; PASS, sync retry → duplicate, conflit payload durable, pull et status central.
- SQLite : PASS, WAL + migrations et journal/outbox survivent à une réouverture ; PASS, acknowledgement atomique conserve le journal ; PARTIAL, les opérations métier locales ne sont pas encore toutes exécutées par projection SQLite.
- CLI migration : PASS, dry-run synthétique avec SHA-256, compteurs, références et zéro modification de la source.
- LiveUpdate : PASS, signature valide ; PASS, signature falsifiée ; PASS, ressource altérée ; PASS, expiration ; PASS, downgrade ; PASS, traversal ; PASS, activation/rollback connu-bon.
- OpenAPI : PASS, YAML parseable et aucune route PDF déclarée.
- Compose : PASS, image release construite sans le `v2/target` local, PostgreSQL healthy, API accessible sur `127.0.0.1:18080/health/live`, migrations appliquées et 30 tables centrales visibles ; volume de test supprimé ensuite.
- CI GitHub : PASS sur la PR #58, run `41474895274`, job `rust` `125377741644`, head `88fc431288fac3ce1491965a418c096adb7c5986`. Le workflow a démarré PostgreSQL 16.4, puis `fmt`, Clippy, la suite workspace et `cargo test --workspace -- --ignored`; tous les tests passent. Les deux annotations restantes concernent uniquement l’avertissement GitHub sur les actions Node.js 20, pas le code Rust.
- V1 Python : `260 passed, 2 failed` sur 262. Les deux échecs sont historiques et temporellement fragiles : `tests/test_invoice_preview.py::test_invoice_confirmation_keeps_the_tariff_used_by_preview` attend le code `RESIDENTIAL-2026-10` alors que la date système est novembre 2026 ; `tests/test_ui_tariff_rules.py::test_configured_future_customer_type_can_be_billed` configure une tarification effective le mois courant puis facture octobre. Ils ne proviennent pas d'une modification V2, mais empêchent un verdict de non-régression V1 totalement vert et doivent être corrigés/confirmés séparément.

## Matrice de recette

`PASS` signifie que le périmètre exact est démontré localement. `NOT_TESTED` signifie qu'un test dédié ou une intégration complète manque. `BLOCKED_EXTERNAL` signifie qu'une source, une clé, un poste, un serveur ou une décision métier réelle est nécessaire.

| ID | Statut | Preuve ou limite |
|---|---|---|
| API-01 | PASS | routes commerciales centrales testées via Axum + PostgreSQL réel, sans frontend |
| API-02 | NOT_TESTED | YAML valide, mais couverture DTO/routes complète à industrialiser |
| API-03 | PARTIAL | le binaire central refuse les bearers arbitraires sans `SEL_API_AUTH_TOKEN`; sessions courtes, rotation, RBAC fin et tests IDOR restent absents |
| API-04 | NOT_TESTED | création/lecture/update testées, pagination/filtre PG absents |
| API-05 | PARTIAL | idempotence persistante testée pour facture, paiement, solaire et sync ; campagne de concurrence multi-clients non exécutée |
| API-06 | PASS | aucun endpoint PDF, JSON document-data et scan de schéma sans stockage fichier |
| FIN-01 | PASS | test `billing_red` : 100 kWh à 0,095 + 16 % = 11,02 USD et deux lignes |
| FIN-02 | PASS | zéro eau sans tarif accepté ; volume positif sans tarif/taxe rejeté par le domaine |
| FIN-03 | NOT_TESTED | branche eau positive codée mais aucun scénario d'approbation tarifaire complet |
| FIN-04 | PASS | PostgreSQL réel : paiement de 3,00 USD, retry exact et balance 8,02 |
| FIN-05 | PASS | vente solaire persistante : une vente, une créance, un plan, deux échéances et retry sans doublon |
| FIN-06 | NOT_TESTED | retard/échéances et affectation de paiement non implémentés complètement |
| FIN-07 | NOT_TESTED | concurrence/reverse append-only non validés |
| FIN-08 | NOT_TESTED | avoir/annulation/compensation non implémentés |
| FIN-09 | NOT_TESTED | Decimal garanti ; matrice USD/CDF/EUR et conversion source non branchée |
| FIN-10 | PARTIAL | le produit solaire actif est exigé et le tarif utility approuvé/daté est lu en central ; matrice exhaustive d'inactifs/catégories/grants expirés à compléter |
| FIN-11 | NOT_TESTED | distinction locale/centrale prévue dans les contrats, numérotation finale non branchée |
| FIN-12 | NOT_TESTED | validation API en mémoire ; transaction PG anti-effet partiel non démontrée |
| SYN-01 | PASS | SQLite WAL, journal et outbox persistent après réouverture ; projection de toutes les opérations métier locale encore incomplète |
| SYN-02 | NOT_TESTED | interruption/reprise de 100 commandes non exécutée |
| SYN-03 | PASS | retry PostgreSQL après commit retourne le résultat durable ; conflit de payload écrit dans `sync_conflicts` |
| SYN-04 | PARTIAL | pull central paginé et cursor testés ; scénario crash SQLite injecté avant commit à compléter |
| SYN-05 | NOT_TESTED | contrainte de période dans le schéma, deux appareils réels non testés |
| SYN-06 | NOT_TESTED | surallocation contrôlée côté API ; deux appareils offline non testés |
| SYN-07 | NOT_TESTED | révision attendue dans l'enveloppe, conflit d'échéancier non implémenté |
| SYN-08 | NOT_TESTED | révocation/grant offline non implémentés |
| SYN-09 | PARTIAL | push/pull et accusé central testés ; worker de drain outbox réseau complet non implémenté |
| SYN-10 | NOT_TESTED | rebootstrap multi-postes sans suppression d'outbox non démontré |
| SYN-11 | PASS | états `LOCAL_PREVIEW`, `CENTRAL_ACCEPTED`, `DUPLICATE`, `CONFLICT` exposés dans les contrats |
| SYN-12 | NOT_TESTED | réservation/stock kit non implémentés |
| UPD-01 | PASS | manifeste déclaratif signé activé sans remplacement d'EXE |
| UPD-02 | PASS | signature, hash, expiration, séquence et chemin incorrects rejetés |
| UPD-03 | PASS | gestionnaire conserve le connu-bon et rollback testés |
| UPD-04 | NOT_TESTED | SafeUpdate natif reste hors de ce backend |
| UPD-05 | BLOCKED_EXTERNAL | aucune clé publique de production fournie ; comportement fail-closed requis |
| MIG-01 | PARTIAL | dry-run portable V1 synthétique vérifié ; copie anonymisée SEL réelle manquante |
| MIG-02 | NOT_TESTED | import PG idempotent réel non implémenté ; seul le manifeste dry-run est livré |
| MIG-03 | BLOCKED_EXTERNAL | comparaison sur données V1 absente |
| MIG-04 | BLOCKED_EXTERNAL | campagne d'erreur/reprise d'import absente |
| MIG-05 | BLOCKED_EXTERNAL | sauvegarde/restauration PG non fournie |
| FILE-01 | PASS | API/OpenAPI sans PDF et document-data JSON |
| FILE-02 | PASS | tests de schémas PG/SQLite sans tokens de stockage de fichier |
| FILE-03 | NOT_TESTED | inspection complète volumes/logs/backups après 100 factures non exécutée |
| FILE-04 | NOT_TESTED | générateur PDF client V2 volontairement hors périmètre backend |
| OPS-01 | PASS | fmt, Clippy et cargo test workspace verts |
| OPS-02 | PASS | CI du commit final démarre PostgreSQL 16.4, applique les migrations au travers des tests et valide les scénarios centraux réels ; le compose de déploiement reste une configuration de test |
| OPS-03 | NOT_TESTED | TLS/reverse proxy, logs PII, backup chiffrée et moindre privilège non validés |
| OPS-04 | BLOCKED_EXTERNAL | poste Windows SEL, VPS, impression et tarifs réels non accessibles/validés |

## Décisions et limitations restantes

1. Ne pas connecter ce backend à des données SEL réelles : les tarifs eau/taxes, les rôles et les paramètres d'organisation restent à fournir/valider.
2. Le chemin central `DATABASE_URL` est maintenant transactionnel pour les opérations testées. Le chemin local persiste les enveloppes, mais l'exécution/projection de toutes les commandes commerciales offline et le worker de drain restent à construire.
3. Ne pas activer LiveUpdate en production tant que la clé Ed25519 publique de production n'est pas provisionnée hors dépôt et que le pipeline de publication/rotation n'est pas validé.
4. Ne pas générer de PDF serveur : le client devra consommer `document-data` et produire le document localement avec un template validé.
5. Le dry-run V1 ne fait encore aucune écriture centrale : l'importeur réel, la réconciliation et la reprise après échec exigent un mapping de données V1 anonymisées.
6. Le jeton de déploiement est une barrière fail-closed minimale. Les sessions utilisateur, la rotation, les permissions par rôle/ressource et l'enrôlement appareil doivent précéder tout usage réel.
7. Les deux tests V1 datés restent à arbitrer avant de déclarer la non-régression globale verte.

## Verdicts

- Revue technique du jalon persistance : **GO limité** pour les parcours PostgreSQL/SQLite démontrés et les tests locaux ; CI du commit final encore requise.
- Pilote Windows/PG/offline : **NO-GO — validations externes et intégration durable manquantes**.
- Livraison commerciale : **NO-GO**.
- Fusion dans `main` : **non effectuée**.
- Release/déploiement : **non effectués**.

La PR doit conserver ce rapport, les logs de CI et les preuves de test. La fermeture des statuts `NOT_TESTED`/`BLOCKED_EXTERNAL` exige une seconde phase explicitement validée, sans modifier ni contourner les protections SafeUpdate ou Windows.
