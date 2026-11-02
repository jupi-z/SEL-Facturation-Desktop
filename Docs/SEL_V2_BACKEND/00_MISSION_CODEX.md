# SEL V2 — mandat Codex : backend autonome d'abord

**Date : 2 novembre 2026. Échéance cible : 3 novembre 2026, 12 h (Africa/Lubumbashi).**  
**Suivi :** https://github.com/jupi-z/SEL-Facturation-Desktop/issues/56  
**Statut :** cahier des charges pour développement; **la fonctionnalité n'est pas encore livrée**.

## Instructions impératives
- **Commencer par un BACKEND Rust autonome et testable sans interface.** Ne pas commencer, prototyper, ou pousser une nouvelle interface Windows/Tauri/React avant la disponibilité en production dûment validée du backend. Le frontend Flask/pywebview existant peut être utilisé **uniquement comme client de test**, avec adaptateur API isolé si besoin.
- **L'application reste utilisable hors connexion.** Créer un moteur Rust partagé entre API centrale et agent local, une persistance SQLite locale, et un protocole de synchronisation avec PostgreSQL. Pas d'accès PostgreSQL direct depuis les postes Windows.
- **PostgreSQL central ne conserve que des données métier structurées.** Ne jamais enregistrer, stocker, servir, journaliser ou sauvegarder de fichier PDF/image/ZIP/BLOB de facture sur le serveur. **Les PDF de factures sont fabriqués exclusivement sur le poste client** depuis le snapshot métier JSON immuable fourni par l'API ou la cache locale, avec template versionné. L'API peut répondre `GET /v1/invoices/{id}/document-data` en JSON mais **n'a aucun endpoint PDF**.
- **Commercial** : eau au m³ et électricité au kWh réunies sur la même facture, 0 m³ toujours affiché; kits solaires avec contrats/échéancier de paiement; paiements partiels de factures ordinaires **sans créer d'échéancier**; créances, règlements et contre-écritures audités, sans double comptage.
- Mises à jour **majoritairement internes** : paramètres, tarifs validés, templates déclaratifs, contenu UI plus tard, livrés et activés après vérification Ed25519/hachages, sans réinstaller l'EXE. Une modification du noyau Rust natif exige une mise à jour binaire vérifiée; ne jamais prétendre au contraire.
- **Ne rien casser** de la v0.3.x. Les PR #47 (eau/élec), #54 (Windows) et #55 (formulaire) sont des travaux existants distincts; faire un état réel avant de réutiliser du code. Conserver compatibilité export `sel-facturation-portable` v1; aucune migration destructive.
- Toutes opérations backend financières via API, idempotentes, testées en concurrence, avec auth RBAC, audit, et messages d'erreur normés.
- Respecter la législation et l'intégration DGI : **aucune prétention à certification fiscale** ou QR fiscal inventé.

## Ambition vs réalité de l'échéance
L'échéance **ne permet pas d'affirmer sans preuve** que toute migration, sécurité, offline multi-poste et installation pilote sont terminées. Codex doit livrer l'ensemble des travaux techniquement réalisables **maintenant**, en priorisant le chemin vertical P0 : client → facturation bi-service → règlement partiel → vente kit solaire + échéancier → offline outbox → reprise centrale. Les autres critères restent suivis avec résultat `PASS/FAIL/BLOCKED/NOT_TESTED` et des preuves. **Une démo n'est pas une mise en production**.

## Structure recommandée (ne pas effacer la racine existante)
```
v2/
  Cargo.toml                  # workspace
  crates/sel-domain/          # règles financières pures, rust_decimal
  crates/sel-contracts/       # DTO, commandes et événements versionnés
  crates/sel-storage/         # PostgreSQL et SQLite, migrations séparées
  crates/sel-sync/            # outbox/inbox, cursors, conflits
  crates/sel-api/             # service central Axum REST/OpenAPI
  crates/sel-local/           # agent local Rust HTTP 127.0.0.1
  crates/sel-cli/             # vérification, migration, réparation
  migrations/postgres/
  migrations/sqlite/
  openapi/
  tests/
  deploy/                     # docker compose de TEST; no secret / no invoice files
```
Codex peut affiner ces packages si justification mesurable. Éviter microservices, Kubernetes, Kafka, CRDT généraliste, front neuf et système comptable complet non demandé.

## Mode de travail
1. Lire tous les documents `Docs/SEL_V2_BACKEND/` en ordre et `AGENTS.md` pour contraintes de préservation, puis auditer main et PR actuelles; **cette décision du propriétaire remplace expressément la restriction ancienne « ne pas remplacer la pile Python » pour V2 seulement**. La V1 reste inchangée.
2. Fournir `ADR`, diagramme, schémas/migrations et **OpenAPI** avant de construire le cœur. Construire ensuite sans demander de nouvelle validation pour chaque détail déjà résolu.
3. PR **technique dédiée, non fusionnée**, changements strictement sous `v2/` et documentation V2, sauf adaptateur/test V1 explicitement identifié.
4. CI `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, migrations PG/SQLite et tests de coupure; commandes et traces reproductibles. Pas de secrets en Git ni CI.
5. Rédiger un rapport final au chemin `Docs/SEL_V2_BACKEND/IMPLEMENTATION_REPORT.md` dans la branche technique, avec preuves réelles, statut de chacun des critères et GO/NO-GO.
6. **Ne pas fusionner dans main, installer chez SEL, publier une release, changer le manifeste updates de production ou déployer sur serveur réel sans validation explicite.** Pour le pilote, disposer des vrais accès VPS, TLS, sauvegardes et paramètres SEL.

## Conditions de sortie
**GO de revue** si le moteur Rust compile, API sans UI fonctionnelle, tests P0 verts sur PostgreSQL + SQLite, sécurité et finance prouvées, 0 fichier facture sur serveur, synchronisation testée avec interruptions, PR et rapport inspectables. **GO de production séparé** après test machine SEL, réseau, restauration, approbation métier et contrôle de sécurité.
