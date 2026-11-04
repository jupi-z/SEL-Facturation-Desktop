# Phase 5 — fonctionnement hors connexion et convergence

## Objectif

Démontrer, avec deux bases SQLite indépendantes et un vrai PostgreSQL, que les opérations hors connexion survivent au redémarrage, sont rejouées sans double effet après perte d'accusé et convergent sur les postes autorisés. Rust reste l'autorité métier ; les conflits sont visibles et auditables.

## Critères de sortie

1. Une facture eau + électricité et un paiement partiel local survivent à la fermeture du premier poste.
2. Le commit métier et le reçu d'événement PostgreSQL précèdent l'accusé remis au client.
3. La perte de l'accusé après commit puis le redémarrage/rejeu retournent un doublon idempotent sans nouvelle écriture financière ni publication.
4. Le second poste récupère les mêmes identifiants, snapshots, factures, affectations, paiements et soldes.
5. Une vente solaire, son contrat, son échéancier et le paiement concurrent sont convergents ; l'obligation et l'encaissement ne sont comptés qu'une fois.
6. Les opérations conflictuelles restent visibles dans le poste initiateur et la décision du serveur est explicite.
7. Les migrations PostgreSQL/SQLite et les tests de panne sont exécutés en CI, sans écrire dans les bases SEL.

## État avant cette phase

- `v2/crates/sel-local/tests/two_device_convergence.rs` contient déjà un scénario ignoré en exécution ordinaire, exécuté par le workflow PostgreSQL dédié : facture combinée, paiement partiel, redémarrage, synchronisation HTTP, lecture dans deux SQLite et PostgreSQL, ventes/contrat solaires et deux paiements concurrents pour la même échéance.
- `v2/crates/sel-local/tests/sync_worker.rs` couvre localement les reprises d'outbox, la perte d'accusé avec un serveur simulé, les curseurs, les projections, les conflits et les projections solaires.
- La lacune ciblée est la jonction des deux preuves : provoquer la perte de réponse **après le commit réel de `sync_push` PostgreSQL**, puis redémarrer et rejouer dans le parcours multi-postes réel.

## Travail de cette phase

- Étendre le test multi-postes PostgreSQL/HTTP existant avec un middleware de panne unique qui remplace la première réponse de `POST /v1/sync/push` après exécution de la route centrale.
- Vérifier que les 13 événements sont déjà dans `sync_inbox`, que le poste garde l'outbox en attente, puis redémarre et rejoue les mêmes événements.
- Vérifier que le rejeu est répondu `Duplicate`, que `sync_inbox` et `central_changes` ne croissent pas, et que les paiements/soldes sont identiques sur PostgreSQL et les deux SQLite.
- Laisser le workflow Backend exécuter le test sur son service PostgreSQL réel, ainsi que la suite standard, `--ignored`, Clippy et rustfmt.

## Limites à suivre séparément

- Le test injecte une réponse HTTP `503` après commit ; il prouve l'indépendance du commit et de l'accusé applicatif, mais ne simule pas encore un reset TCP réel ni une coupure prolongée de réseau Windows.
- Le test de worker automatique garde un endpoint simulé ; le parcours d'interface Windows « reconnexion détectée » sera couvert avec la phase frontend et sur un poste représentatif.
- La recette réseau/matérielle SEL reste `BLOCKED_EXTERNAL` tant qu'un poste et un réseau SEL ne sont pas disponibles.
