# SEL V2 Backend — rapport d’implémentation

Date de contrôle : 3 novembre 2026, Africa/Lubumbashi

Branche : `monarque/sel-v2-backend`

PR : [#58](https://github.com/jupi-z/SEL-Facturation-Desktop/pull/58)

Commit contrôlé : `0dcce2f` (`feat(v2): add audited payment reversals`)
Périmètre : backend Rust/API, PostgreSQL, SQLite locale, synchronisation et LiveUpdate. Aucun nouveau frontend, aucune fusion dans `main`, aucune release et aucun déploiement réel.

## Résumé exécutif

Le backend Rust dispose maintenant d’un parcours démontré de bout en bout pour une facture eau + électricité et un paiement partiel : deux bases SQLite indépendantes créent et conservent leurs identifiants hors ligne, un serveur HTTP Rust persiste les commandes dans PostgreSQL, puis le second poste récupère les mêmes données. Le scénario comprend aussi une vente de kit solaire, son échéancier, une reprise après redémarrage et une tentative de double encaissement.

Le dernier correctif (`08ce31d`) unifie l’empreinte des factures. Le format canonique est versionné, exclut les statuts de synchronisation et encode les montants avec six décimales fixes, ce qui neutralise la différence d’échelle entre SQLite (`0.095`) et PostgreSQL (`0.095000`). Un test de contrat vérifie explicitement cette invariance.

La CI GitHub #23 était verte sur le commit précédent `1ba7ca9`; une nouvelle exécution est requise pour le commit courant. Cela valide le périmètre technique couvert par les tests précédents, tandis que les nouveaux tests ciblés sont également exécutés localement. Le backend reste **NO-GO pour une exploitation commerciale** : l’authentification complète/RBAC, les autorisations offline signées, l’import V1 avec écriture, la sauvegarde/restauration PostgreSQL, certaines opérations comptables et la recette sur poste SEL restent non validés ou incomplets.

## Corrections réalisées sur ce jalon

- Empreintes de factures déterministes et partagées entre SQLite, API et PostgreSQL.
- Canonicalisation des `Decimal` à six décimales dans le snapshot versionné ; les champs financiers sont distincts des statuts `LOCAL_PENDING`/`CENTRAL_ACCEPTED`.
- Réconciliation d’une facture locale confirmée centralement sans réécrire ses identifiants, lignes, montants ou snapshot.
- Réconciliation idempotente des paiements et de leurs affectations, sans second paiement.
- Contre-écritures de paiements append-only : renversement partiel ou total, verrouillage du paiement, somme cumulée bornée, acteur actif vérifié, audit et `central_changes` dans la même transaction.
- Soldes PostgreSQL et SQLite calculés sur les allocations nettes après renversements, avec rejeu idempotent de l’événement `PAYMENT_REVERSED` et projection locale contrôlée.
- Validation globale des affectations par créance, verrouillage PostgreSQL déterministe et rejet d’une surallocation, y compris deux affectations répétées vers la même créance.
- Publication centrale récupérable après commit métier avant accusé, avec outbox/replay idempotent.
- Projections SQLite solaires : vente, lignes, créance, contrat, plan et échéances avec identifiants stables et relations conservées.
- Entités centrales non supportées par une projection locale enregistrées comme conflits durables au lieu d’être ignorées silencieusement.
- Facture de services toujours composée des deux lignes : électricité en kWh et eau en m³ ; `0 m³` est conservé et visible.
- Rejet explicite des effets locaux refusés, sans suppression destructive de l’audit ou de l’outbox.

## Preuve décisive de convergence

Test : `v2/crates/sel-local/tests/two_device_convergence.rs`
Commande locale exécutée avec PostgreSQL réel :

```text
TEST_DATABASE_URL=postgres://sel:postgres@172.17.0.2:5432/sel_v2_test
cargo test -p sel-local --test two_device_convergence -- --ignored --nocapture
test two_sqlite_devices_converge_through_real_postgres_and_http ... ok
test result: ok. 1 passed; 0 failed
```

Le scénario vérifie :

1. client, facture combinée et paiement partiel créés sur SQLite 1 ;
2. réouverture de SQLite 1 avant synchronisation ;
3. serveur HTTP réel `sel-api` avec repository PostgreSQL réel ;
4. persistance centrale puis accusé ;
5. récupération des données sur SQLite 2 ;
6. égalité des identifiants, lignes, totaux et empreintes ;
7. vente solaire, une créance, un contrat et deux échéances ;
8. rejeu idempotent des projections solaires ;
9. deux tentatives de paiement de 70 USD sur une créance de 120 USD : une seule est acceptée, la seconde est `REJECTED`, le solde reste 50 USD ;
10. absence de double comptabilisation dans PostgreSQL et SQLite 2.

Ce test ne constitue pas encore une simulation exhaustive de panne physique à chaque instruction réseau ni une concurrence multi-processus réellement simultanée ; ces points restent marqués comme non testés ci-dessous.

## Résultats de validation

### Validation locale exécutée

- `cargo test -p sel-contracts` : **PASS**, 2 tests unitaires et contrat OpenAPI.
- `cargo clippy --workspace --all-targets -- -D warnings` : **PASS** dans le conteneur Rust 1.85.
- test d’intégration deux postes : **PASS** avec PostgreSQL réel, après correction de l’empreinte et des attentes de solde.
- test API PostgreSQL réel `utility_issue_is_atomic_idempotent_and_survives_restart` : **PASS**, avec renversement `1.00` puis `2.00`, rejeu idempotent, soldes nets, audit et publications centrales.
- test SQLite local `local_commercial_operations_are_persisted_and_offline_pending` : **PASS**, avec renversement hors ligne durable et solde local net.
- test de synchronisation `payment_sync_is_applied_before_the_central_ack_and_is_idempotent` : **PASS**, avec événement `PAYMENT_REVERSED` accepté puis rejoué sans doublon.
- `git diff --check` : **PASS**.
- aucun secret, PDF, image ou facture binaire n’a été ajouté au serveur par les changements de ce jalon.

### CI GitHub

Run [#23 — 41588138616](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41588138616), job `rust` `125758328489` : **SUCCESS** sur le commit précédent `1ba7ca9`. Une nouvelle exécution CI est attendue sur `0dcce2f`.

- `cargo fmt --all -- --check` : **PASS** ;
- `cargo clippy --workspace --all-targets -- -D warnings` : **PASS** ;
- `cargo test --workspace` : **PASS** ;
- `cargo test --workspace -- --ignored` avec PostgreSQL 16.4 : **PASS** ;
- test `two_sqlite_devices_converge_through_real_postgres_and_http` : **PASS** ;
- nettoyage du conteneur PostgreSQL de CI : **PASS**.

La CI est une validation Linux/PostgreSQL de test. Elle ne remplace pas la recette Windows, l’impression ni la validation d’un serveur SEL.

## Matrice de recette

`PASS` signifie que le périmètre exact est démontré. `PARTIAL` signifie qu’une partie seulement est prouvée. `NOT_TESTED` signifie qu’une preuve dédiée manque. `BLOCKED_EXTERNAL` nécessite une donnée, une clé, un serveur ou une validation externe.

| ID | Statut | Preuve ou limite |
|---|---|---|
| API-01 | PASS | routes commerciales centrales testées via Axum + PostgreSQL réel |
| API-02 | PARTIAL | OpenAPI valide et tests ciblés ; couverture exhaustive de tous les DTO/routes à industrialiser |
| API-03 | PARTIAL | bearer de production exigé ; sessions courtes, rotation, RBAC fin et tests IDOR non terminés |
| API-04 | PARTIAL | création/lecture/modification testées ; pagination et filtres centraux incomplets |
| API-05 | PASS limité | idempotence facture, paiement, solaire et sync démontrée ; concurrence multi-processus non exhaustive |
| API-06 | PASS | aucune route PDF ; `document-data` JSON côté client ; aucun stockage de fichier |
| FIN-01 | PASS | facture électrique + eau, `0 m³` visible, Decimal et arrondi testés |
| FIN-02 | PASS | eau positive refusée sans tarif/taxe validés ; eau nulle conservée |
| FIN-03 | NOT_TESTED | scénario complet d’un tarif eau positif approuvé manque |
| FIN-04 | PASS | paiement partiel, solde et retry sans doublon |
| FIN-05 | PASS limité | vente solaire, créance, contrat et deux échéances persistés et projetés |
| FIN-06 | PARTIAL | affectation à échéance et rejet de surallocation testés ; retards et recouvrement complets manquent |
| FIN-07 | PARTIAL | contre-écritures de paiements append-only, audit, solde net et rejeu testés ; remboursements externes et leur rapprochement restent à finaliser |
| FIN-08 | NOT_TESTED | avoirs, annulations et compensations non finalisés |
| FIN-09 | PARTIAL | Decimal et snapshot multi-store ; matrice complète USD/CDF et taux approuvés à compléter |
| FIN-10 | PARTIAL | tarif utility approuvé/daté et produit solaire actif contrôlés ; matrice exhaustive des catégories à compléter |
| SYN-01 | PASS | test deux SQLite + PostgreSQL réel, reprise après ouverture et projections idempotentes |
| SYN-02 | PARTIAL | retry/replay démontré ; coupure injectée à chaque frontière réseau non exhaustive |
| SYN-03 | PASS limité | publication et accusé récupérables après commit simulé ; panne physique réelle non injectée |
| SYN-04 | PARTIAL | pull et curseur durable testés ; crash SQLite avant commit à compléter |
| SYN-05 | NOT_TESTED | conflit de facture concurrente sur même période entre deux postes non démontré |
| SYN-06 | PARTIAL | surallocation centrale protégée ; paiements réellement simultanés entre deux postes à compléter |
| SYN-07 | NOT_TESTED | conflit de contrat/échéancier concurrent non implémenté complètement |
| SYN-08 | NOT_TESTED | autorisations offline signées, expiration et limites non finalisées |
| SYN-09 | PASS limité | worker push/pull et reprise de l’outbox codés ; réseau réellement intermittent non validé sur Windows |
| SYN-10 | NOT_TESTED | rebootstrap multi-postes sans suppression d’outbox non démontré |
| UPD-01 | PASS | manifeste LiveUpdate déclaratif signé sans remplacement natif |
| UPD-02 | PASS | signature Ed25519, SHA-256, expiration, séquence et chemins invalides rejetés |
| UPD-03 | PASS | activation et rollback connu-bon testés |
| UPD-04 | NOT_TESTED | SafeUpdate natif hors périmètre de cette validation backend |
| UPD-05 | BLOCKED_EXTERNAL | clé publique Ed25519 de production absente ; fail-closed conservé |
| MIG-01 | PARTIAL | dry-run V1 vérifié sur format synthétique ; import avec écriture non livré |
| MIG-02 | NOT_TESTED | import PostgreSQL idempotent réel non livré |
| MIG-03 | BLOCKED_EXTERNAL | données V1 anonymisées de rapprochement absentes |
| MIG-04 | BLOCKED_EXTERNAL | campagne reprise/rollback d’import non réalisée |
| MIG-05 | BLOCKED_EXTERNAL | sauvegarde/restauration PostgreSQL de production non fournie |
| FILE-01 | PASS | serveur/API ne déclarent ni ne stockent de PDF |
| FILE-02 | PASS | schémas et tests ne contiennent pas de champ de fichier de facture |
| FILE-03 | NOT_TESTED | audit des volumes/logs/backups après charge non réalisé |
| FILE-04 | NOT_TESTED | générateur PDF client V2 volontairement hors backend |
| OPS-01 | PASS limité | formatage, Clippy et tests workspace verts en CI #23 sur le commit précédent ; nouveau commit en attente de CI |
| OPS-02 | PASS | PostgreSQL de test en CI avec migrations et tests ignorés verts |
| OPS-03 | NOT_TESTED | TLS, reverse proxy, PII, sauvegardes chiffrées et moindre privilège non validés |
| OPS-04 | BLOCKED_EXTERNAL | poste Windows SEL, impression et serveur réel non accessibles pour recette |

## Lacunes et décisions restant à traiter

1. Remplacer la garde par jeton de développement par utilisateurs, sessions à durée limitée, rotation/révocation, RBAC et contrôle par organisation/ressource.
2. Finaliser les autorisations offline signées Ed25519, leur expiration, leurs plafonds d’opérations et leur enrôlement d’appareil.
3. Étendre le local aux corrections, annulations, remboursements, contre-écritures, retards et recouvrement solaires sans supprimer l’historique.
4. Implémenter et tester l’import V1 transactionnel avec sauvegarde, simulation, idempotence, rapprochement et reprise sur données anonymisées fournies par SEL.
5. Documenter et exercer la sauvegarde/restauration PostgreSQL, TLS, rotation des secrets et le déploiement à privilèges minimaux.
6. Provisionner la clé publique Ed25519 de production hors dépôt avant d’activer LiveUpdate ; aucune signature de test ne doit être promue.
7. Réaliser la recette sur un PC Windows SEL : installation, fonctionnement hors connexion, synchronisation, impression, sauvegarde/restauration et validation métier des tarifs eau/taxes.

## Verdicts

- Revue technique du jalon persistance/synchronisation : **GO limité** ; les tests Rust et le scénario de convergence de référence sont verts.
- Pilote Windows/PostgreSQL/offline : **NO-GO** ; validation matérielle et opérationnelle non réalisée.
- Livraison commerciale : **NO-GO** ; sécurité de production, migration, exploitation et opérations comptables restantes.
- Fusion dans `main` : **NON EFFECTUÉE**.
- Release, publication d’installateur et déploiement réel : **NON EFFECTUÉS**.

La PR #58 est prête pour revue technique sur le périmètre démontré, mais ne doit pas être annoncée comme backend de production tant que les statuts `PARTIAL`, `NOT_TESTED` et `BLOCKED_EXTERNAL` critiques ne sont pas fermés avec des preuves correspondantes.
