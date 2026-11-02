# SEL V2 — backend Rust autonome / offline-first

**Dossier normatif maître daté du 2 novembre 2026.** Mission de développement à Codex : https://github.com/jupi-z/SEL-Facturation-Desktop/issues/56. Ce dossier est **documentaire uniquement** et ne modifie pas le moteur actuel.

**Point d'entrée impératif :** [CODEX_SEL_V2_BACKEND_START_HERE.md](../../CODEX_SEL_V2_BACKEND_START_HERE.md).

## Ordre de lecture
1. [00 Mission Codex](00_MISSION_CODEX.md)
2. [01 Architecture et ADR](01_ARCHITECTURE_ET_ADR.md)
3. [02 Modèle de données et invariants](02_MODELE_DE_DONNEES_ET_INVARIANTS.md)
4. [03 API et autonomie](03_CONTRATS_API_ET_AUTONOMIE.md)
5. [04 Synchronisation offline et conflits](04_SYNCHRONISATION_OFFLINE_ET_CONFLITS.md)
6. [05 LiveUpdate, sécurité et données](05_LIVEUPDATE_SECURITE_ET_DONNEES.md)
7. [06 Migration et déploiement](06_MIGRATION_DEPLOIEMENT_ET_EXPLOITATION.md)
8. [07 Priorités 23 h](07_PRIORITES_23H_ET_LIVRABLES.md)
9. [08 Tests et preuves](08_MATRICE_TESTS_ET_PREUVES.md)

## Contrat non négociable
- **Le backend Rust doit être autonome, testable sans frontend, et être la priorité absolue.**
- PostgreSQL central + SQLite local, commandes financières, API HTTP, sync durable et réconciliation.
- Eau (m³) + électricité (kWh) : une seule facture, 0 m³ toujours visible.
- Kits solaires avec échéancier **≠** factures avec paiement partiel et dette simple.
- **ZÉRO fichier facture sur le serveur** (aucun PDF/images de facture, fichier binaire, BLOB ou lien fichier dans PostgreSQL, stockage objet, disque, cache, logs ou backups). Seulement des **données structurées** et snapshots figés. PDF généré exclusivement côté client.
- LiveUpdate sans nouvel EXE pour configuration et ressources compatibles; SafeUpdate pour le code Rust/Windows natif. Aucun bypass de signature/Windows.
- Ne pas lancer le développement du nouveau frontend avant la production du backend validée; le frontend existant n'est qu'un adaptateur **de tests**.
- PR technique non fusionnée et aucune release ou déploiement sans accord explicite.

## Préséance des consignes
Le fichier historique `AGENTS.md` interdit de changer la pile Flask/SQLite **sans autorisation**. L'utilisateur a **maintenant autorisé explicitement Rust/PostgreSQL pour SEL V2**, sous le nouvel espace `v2/`. Les protections de données, compatibilité et revue de PR d'AGENTS.md restent impératives. Les dossiers `Docs/SEL_RELEASE_GATE_V1/` et `Docs/SEL_SAFEUPDATE_V1/` demeurent références pour le fonctionnement historique et les mises à jour.
