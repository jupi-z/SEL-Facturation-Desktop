# SEL V2 Backend — rapport d'implémentation

Date de contrôle : 2 novembre 2026, Africa/Lubumbashi  
Branche : `monarque/sel-v2-backend`  
Commits de référence : implémentation `b17c1d5`, code vérifié par CI `7d7d329`, dernier commit documentaire de la PR `4dc5c16`.  
Périmètre : backend Rust/API, migrations, synchronisation et LiveUpdate. Aucun nouveau frontend, aucune fusion dans `main`, aucune release et aucun déploiement réel.

## Résumé exécutif

Le dépôt contient maintenant un workspace Rust autonome, les contrats JSON versionnés, un moteur de calcul Decimal pour l'électricité et l'eau, une validation solaire, des migrations PostgreSQL/SQLite, les premiers endpoints Axum, un protocole de sync idempotent de référence, un agent local loopback et un vérificateur LiveUpdate Ed25519/SHA-256 avec anti-rollback et retour arrière.

Les preuves locales sont vertes pour le périmètre effectivement implémenté : `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, migrations SQLite répétées, parsing YAML OpenAPI, calcul utility à deux lignes, idempotence sync, parcours vente/échéancier solaire et tests adversariaux LiveUpdate. Le compose PostgreSQL de test a été construit, démarré, migré et interrogé sur `/health/live`; PostgreSQL contenait 30 tables de schéma avant nettoyage du volume de test.

Le verdict n'est pas un GO de production. L'API HTTP actuelle reste une fondation de contrats avec état métier en mémoire ; l'ouverture de PostgreSQL et l'application des migrations sont réalisées au démarrage quand `DATABASE_URL` est fourni, mais les écritures API ne sont pas encore branchées sur des repositories PostgreSQL transactionnels. L'authentification/RBAC de production, les grants offline signés, la durabilité SQLite de l'agent, l'import V1 et les tests Windows/PG de reprise restent à réaliser ou à valider.

## Corrections et livrables

- `v2/crates/sel-domain` : montants `rust_decimal::Decimal`, arrondi à deux décimales, règles de non-négativité, facture utility toujours composée de `ELECTRICITY` et `WATER`, eau à `0 m³` visible sans tarif inventé, eau positive bloquée sans tarif/taxe configurés, échéancier solaire dont la somme doit égaler exactement le principal.
- `v2/crates/sel-contracts` : DTO API, version de protocole/document, erreurs Problem Details, document-data JSON, paiements, sync et contrats solaires.
- `v2/crates/sel-storage` : ouverture/migrations PostgreSQL et SQLite via SQLx ; SQLite active WAL et contient journal de commandes, outbox, inbox, curseurs, conflits et projections.
- `v2/migrations/postgres/0001_init.sql` et `v2/migrations/sqlite/0001_init.sql` : modèle structuré clients, services, produits, tarifs, taxes, FX, ventes, factures, lignes, créances, plans, paiements, audit, idempotence et sync. Aucun champ/stockage de fichier de facture.
- `v2/crates/sel-api` : health, capacités, clients, `If-Match`/ETag de modification, preview/issue utility, paiements et soldes de créances, document-data JSON, ventes solaires et activation d'échéancier, sync push/pull/status, métadonnées de configuration/ressources. Les opérations financières exigent `Idempotency-Key` lorsqu'elles écrivent.
- `v2/crates/sel-sync` : déduplication `(device_id,event_id)`, contrôle SHA-256 du payload, résultats `ACCEPTED`/`DUPLICATE`/`CONFLICT`, curseur central, manifeste LiveUpdate signé Ed25519, expiration, séquence anti-rollback, chemins relatifs, taille/hash des ressources, activation et rollback connu-bon.
- `v2/crates/sel-local` : endpoints loopback `/local/v1/sync/status`, `/sync/trigger` et `/updates/status`, sans proxy aveugle ni affirmation de synchronisation centralisée.
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
```

Résultats :

- Rust : PASS, formatage vérifié ; PASS, Clippy sans warning ; PASS, tous les tests workspace (12 tests d'intégration/domaines, doctests sans échec).
- API : PASS, facture utility JSON à deux lignes avec eau zéro ; PASS, modification client protégée par révision ; PASS, vente solaire et activation 50+50 ; PASS, sync retry → duplicate et pull d'une modification.
- SQLite : PASS, migrations appliquées deux fois en mémoire et tables locales/outbox présentes ; PASS, scan des schémas sans champs de fichier de facture.
- LiveUpdate : PASS, signature valide ; PASS, signature falsifiée ; PASS, ressource altérée ; PASS, expiration ; PASS, downgrade ; PASS, traversal ; PASS, activation/rollback connu-bon.
- OpenAPI : PASS, YAML parseable et aucune route PDF déclarée.
- Compose : PASS, image release construite sans le `v2/target` local, PostgreSQL healthy, API accessible sur `127.0.0.1:18080/health/live`, migrations appliquées et 30 tables centrales visibles ; volume de test supprimé ensuite.
- CI GitHub : PASS sur la PR #58, workflow `SEL V2 Backend`, run `41452693324`, job `rust` `125301531864`, commit testé `7d7d32909d69fed49fca56fe8e7dda1c825557af`. Les trois étapes `fmt`, `clippy -D warnings` et `cargo test --workspace` sont vertes. Les annotations GitHub concernent uniquement la migration Node.js 20 des actions et le futur label `ubuntu-latest`, pas le code Rust.
- V1 Python : `260 passed, 2 failed` sur 262. Les deux échecs sont historiques et temporellement fragiles : `tests/test_invoice_preview.py::test_invoice_confirmation_keeps_the_tariff_used_by_preview` attend le code `RESIDENTIAL-2026-10` alors que la date système est novembre 2026 ; `tests/test_ui_tariff_rules.py::test_configured_future_customer_type_can_be_billed` configure une tarification effective le mois courant puis facture octobre. Ils ne proviennent pas d'une modification V2, mais empêchent un verdict de non-régression V1 totalement vert et doivent être corrigés/confirmés séparément.

## Matrice de recette

`PASS` signifie que le périmètre exact est démontré localement. `NOT_TESTED` signifie qu'un test dédié ou une intégration complète manque. `BLOCKED_EXTERNAL` signifie qu'une source, une clé, un poste, un serveur ou une décision métier réelle est nécessaire.

| ID | Statut | Preuve ou limite |
|---|---|---|
| API-01 | NOT_TESTED | API autonome démontrée en mémoire/compose ; branchement métier PG non complet |
| API-02 | NOT_TESTED | YAML valide, mais couverture DTO/routes complète à industrialiser |
| API-03 | NOT_TESTED | bearer de développement seulement ; RBAC, tokens courts et IDOR production absents |
| API-04 | NOT_TESTED | création/lecture/update testées, pagination/filtre PG absents |
| API-05 | NOT_TESTED | idempotence métier testée sur sync, campagne 50 POST et concurrence non exécutées |
| API-06 | PASS | aucun endpoint PDF, JSON document-data et scan de schéma sans stockage fichier |
| FIN-01 | PASS | test `billing_red` : 100 kWh à 0,095 + 16 % = 11,02 USD et deux lignes |
| FIN-02 | PASS | zéro eau sans tarif accepté ; volume positif sans tarif/taxe rejeté par le domaine |
| FIN-03 | NOT_TESTED | branche eau positive codée mais aucun scénario d'approbation tarifaire complet |
| FIN-04 | NOT_TESTED | paiement API partiel et balance existent, scénario de solde 8,02 non exécuté |
| FIN-05 | NOT_TESTED | une vente et une créance sont testées, deux paiements et solde 50 non testés |
| FIN-06 | NOT_TESTED | retard/échéances et affectation de paiement non implémentés complètement |
| FIN-07 | NOT_TESTED | concurrence/reverse append-only non validés |
| FIN-08 | NOT_TESTED | avoir/annulation/compensation non implémentés |
| FIN-09 | NOT_TESTED | Decimal garanti ; matrice USD/CDF/EUR et conversion source non branchée |
| FIN-10 | NOT_TESTED | catalogue approuvé/inactif et grant expiré non intégrés au repository central |
| FIN-11 | NOT_TESTED | distinction locale/centrale prévue dans les contrats, numérotation finale non branchée |
| FIN-12 | NOT_TESTED | validation API en mémoire ; transaction PG anti-effet partiel non démontrée |
| SYN-01 | NOT_TESTED | agent local démarre, mais SQLite/outbox durable n'est pas câblée |
| SYN-02 | NOT_TESTED | interruption/reprise de 100 commandes non exécutée |
| SYN-03 | NOT_TESTED | déduplication en mémoire testée, coupure après commit PG non exécutée |
| SYN-04 | NOT_TESTED | pull paginé de référence présent, crash transactionnel SQLite non testé |
| SYN-05 | NOT_TESTED | contrainte de période dans le schéma, deux appareils réels non testés |
| SYN-06 | NOT_TESTED | surallocation contrôlée côté API ; deux appareils offline non testés |
| SYN-07 | NOT_TESTED | révision attendue dans l'enveloppe, conflit d'échéancier non implémenté |
| SYN-08 | NOT_TESTED | révocation/grant offline non implémentés |
| SYN-09 | NOT_TESTED | push/pull élémentaire testé, drain outbox durable non testé |
| SYN-10 | NOT_TESTED | rebootstrap sans suppression outbox non implémenté |
| SYN-11 | PASS | états `LOCAL_PREVIEW`, `CENTRAL_ACCEPTED`, `DUPLICATE`, `CONFLICT` exposés dans les contrats |
| SYN-12 | NOT_TESTED | réservation/stock kit non implémentés |
| UPD-01 | PASS | manifeste déclaratif signé activé sans remplacement d'EXE |
| UPD-02 | PASS | signature, hash, expiration, séquence et chemin incorrects rejetés |
| UPD-03 | PASS | gestionnaire conserve le connu-bon et rollback testés |
| UPD-04 | NOT_TESTED | SafeUpdate natif reste hors de ce backend |
| UPD-05 | BLOCKED_EXTERNAL | aucune clé publique de production fournie ; comportement fail-closed requis |
| MIG-01 | BLOCKED_EXTERNAL | copie V1 anonymisée non fournie |
| MIG-02 | BLOCKED_EXTERNAL | importeur V1 idempotent à construire après validation du mapping réel |
| MIG-03 | BLOCKED_EXTERNAL | comparaison sur données V1 absente |
| MIG-04 | BLOCKED_EXTERNAL | campagne d'erreur/reprise d'import absente |
| MIG-05 | BLOCKED_EXTERNAL | sauvegarde/restauration PG non fournie |
| FILE-01 | PASS | API/OpenAPI sans PDF et document-data JSON |
| FILE-02 | PASS | tests de schémas PG/SQLite sans tokens de stockage de fichier |
| FILE-03 | NOT_TESTED | inspection complète volumes/logs/backups après 100 factures non exécutée |
| FILE-04 | NOT_TESTED | générateur PDF client V2 volontairement hors périmètre backend |
| OPS-01 | PASS | fmt, Clippy et cargo test workspace verts |
| OPS-02 | PASS | compose construit, démarre, migre PostgreSQL et health répond ; service métier mémoire |
| OPS-03 | NOT_TESTED | TLS/reverse proxy, logs PII, backup chiffrée et moindre privilège non validés |
| OPS-04 | BLOCKED_EXTERNAL | poste Windows SEL, VPS, impression et tarifs réels non accessibles/validés |

## Décisions et limitations restantes

1. Ne pas connecter ce prototype à des données SEL réelles : les tarifs eau/taxes et les rôles restent à fournir/valider.
2. Ne pas présenter `sel-api` comme prêt pour production tant que l'état mémoire n'est pas remplacé par des transactions PostgreSQL et que l'agent local n'exécute pas les commandes SQLite + journal + outbox atomiquement.
3. Ne pas activer LiveUpdate en production tant que la clé Ed25519 publique de production n'est pas provisionnée hors dépôt et que le pipeline de publication/rotation n'est pas validé.
4. Ne pas générer de PDF serveur : le futur client devra consommer `document-data` et produire le document localement avec un template validé.
5. Les deux tests V1 datés doivent être arbitrés avant de déclarer la non-régression globale verte.

## Verdicts

- Revue technique du socle : **GO limité au socle V2 et à la revue de code**, avec les limites ci-dessus.
- Pilote Windows/PG/offline : **NO-GO — validations externes et intégration durable manquantes**.
- Livraison commerciale : **NO-GO**.
- Fusion dans `main` : **non effectuée**.
- Release/déploiement : **non effectués**.

La PR doit conserver ce rapport, les logs de CI et les preuves de test. La fermeture des statuts `NOT_TESTED`/`BLOCKED_EXTERNAL` exige une seconde phase explicitement validée, sans modifier ni contourner les protections SafeUpdate ou Windows.
