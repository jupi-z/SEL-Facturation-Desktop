# Plan de réalisation priorisé — fenêtre cible 23 heures

**Fenêtre** : du 2 novembre 2026 vers 13:00 au 3 novembre 2026 à 12:00, heure Africa/Lubumbashi. Cette fenêtre est une cible **non garantie** de réalisation technique, **pas une promesse de sécurité ou d'homologation**. Codex peut organiser le travail en sous-tâches sans conflits, mais doit livrer des preuves et faire un bilan à l'échéance.

## Gates séquentiels (le front neuf est interdit)
| Phase | Budget indicatif | Sortie concrète / gate |
|---|---|---|
| P0 — cadrage, architecture | 0–2 h | ADR, ERD, règles métier/credit, OpenAPI, invariants, inventaire V1 + PR; contrat « zéro fichier facture sur serveur » |
| P1 — ossature Rust/PostgreSQL | 2–7 h | Cargo workspace, migrations PG + SQLite, API Axum autonome, auth dev/test, clients, catalogues, health checks, `cargo test` |
| P2 — vertical commercial | 7–13 h | facture bi-service 0 m³, kits solaires + plan échéances, paiement partiel de facture, ledger et créances, tests monétaires |
| P3 — agent local, offline sync | 13–18 h | API locale Rust, SQLite outbox, push/pull idempotent, restart/panne et 2 appareils, gestion conflit |
| P4 — LiveUpdate et compatibilité | 18–20 h | manifeste signé de **test**, configuration versionnée, activation et rollback, document-data JSON sans PDF serveur, script de migration dry-run |
| P5 — audit et preuves | 20–23 h | tests, CI, OpenAPI, docker compose test, sauvegarde/restore, rapport GO/NO-GO et PR |

**Priorité en arbitrage de temps :** vérifier l'intégrité monétaire, l'authentification et la déduplication avant les fonctionnalités secondaires. Si P3/P4 ne passent pas leurs tests de sécurité, la fonction doit rester désactivée en production sans fausse déclaration. Ne pas rendre "PASS" un critère non vérifié.

## Definition of done, ordre de vérification
1. `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, migrations et tests API PostgreSQL réels, SQLite offline.
2. API écoute sans UI, OpenAPI parsée/validée, un client peut créer factures, ventes solaires, paiements, obtenir solde. Pas de dépendance au serveur Flask.
3. Coupure réseau puis 2 processus/instances simulateurs, double commande réessayée et conflits : pas de double création/encaissement, logs anonymisés et rattrapage.
4. **Facturation bi-service obligatoire** pour les nouvelles factures; tarif eau positif bloqué sans validation SEL; zéro autorisé sans inventer les paramètres inconnus.
5. **Un seul montant dû** pour la vente du kit, calendrier de paiements distinct des paiements partiels hors contrat. Règles arrondis et monnaie homogènes.
6. Aucun PDF sur serveur ni endpoint PDF, API document-data JSON; création du PDF local via outil Windows/test **sans nouveau frontend** si environnement disponible. En l'absence de Windows, NOT_TESTED et voie future documentée.
7. No secrets or real clients; `docker compose` local ou environnement test reproduit; test restauration et état des accès distants documentés.
8. La clé Ed25519 de production / certificat Authenticode Windows / VPS officiel sont des **pré-requis externes**, pas des ressources inventables.
9. Générer `Docs/SEL_V2_BACKEND/IMPLEMENTATION_REPORT.md` sur la PR avec SHA, commandes, logs, versions, tests, limites et décision `GO_REVIEW | NO_GO_PROD`. Le dernier SHA testé doit être identique au HEAD de la PR.
10. Conserver les documents normatifs sur `main` et toute implémentation V2 dans PR distincte. **Ne pas développer Tauri/React ni reconstruire le frontend avant backend validé en production.**

## Interdictions et limites d'autonomie
- Pas de publication, fusion main, activation de mise à jour client, installation PC SEL ni modification de serveur réel sans accord du propriétaire.
- Pas de factures réelles, taxes / tarifs inventés, données financières fabriquées sauf **fixtures de test explicitement étiquetées**.
- Pas de bypass de signature, d'UAC/SmartScreen, ni de stockage PDF serveur pour accélérer.
- Pas de dépendance à un SaaS opaque pour les paiements, ni de service de sync tiers en production sans analyse de conformité.
- **Rapport même si tâches incomplètes.** Progression `PASS/total` par gate et globale, avec `BLOCKED_EXTERNAL` distinct; ne jamais annoncer réussite à 100 % quand le système n'est pas passé en production.

## Exemple d'une preuve E2E minimale sans UI
`curl /health/ready` → création client → preview kWh=100/m³=0 → facture JSON → paiement partiel 3 USD → solde 8,02 USD dans fixture électrique 11,02 USD; second scénario : vente kit à 100 USD, échéances 50+50, deux paiements 20+30, solde dû 50 USD (uniquement si la sale a une obligation de 100); statut de retard calculé sur date. Lancer le même scénario local offline puis push/pull, et prouver déduplication/reprise.
