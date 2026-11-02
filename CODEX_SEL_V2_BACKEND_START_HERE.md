# Codex — NOUVELLE MISSION PRIORITAIRE SEL V2 BACKEND

> **2 novembre 2026. Échéance cible : 3 novembre 2026, 12 h (Africa/Lubumbashi).**
> Le propriétaire autorise la conception et l'implémentation de SEL V2, Rust/API-first, **backend d'abord**. Ce document est la **source de vérité** pour la V2 et n'autorise aucune modification sauvage de la V1.

**Dépôt :** https://github.com/jupi-z/SEL-Facturation-Desktop  
**Suivi :** https://github.com/jupi-z/SEL-Facturation-Desktop/issues/56  
**Dossier :** [Docs/SEL_V2_BACKEND/README.md](Docs/SEL_V2_BACKEND/README.md)

## Ordre de lecture obligatoire
1. [Mandat Codex](Docs/SEL_V2_BACKEND/00_MISSION_CODEX.md)
2. [Architecture et ADR](Docs/SEL_V2_BACKEND/01_ARCHITECTURE_ET_ADR.md)
3. [Modèle PostgreSQL/SQLite et invariants](Docs/SEL_V2_BACKEND/02_MODELE_DE_DONNEES_ET_INVARIANTS.md)
4. [Contrats API sans UI](Docs/SEL_V2_BACKEND/03_CONTRATS_API_ET_AUTONOMIE.md)
5. [Sync offline et conflits](Docs/SEL_V2_BACKEND/04_SYNCHRONISATION_OFFLINE_ET_CONFLITS.md)
6. [LiveUpdate et sécurité](Docs/SEL_V2_BACKEND/05_LIVEUPDATE_SECURITE_ET_DONNEES.md)
7. [Migration et exploitation](Docs/SEL_V2_BACKEND/06_MIGRATION_DEPLOIEMENT_ET_EXPLOITATION.md)
8. [Gates 23 h](Docs/SEL_V2_BACKEND/07_PRIORITES_23H_ET_LIVRABLES.md)
9. [Matrice de tests et preuves](Docs/SEL_V2_BACKEND/08_MATRICE_TESTS_ET_PREUVES.md)
10. [README du dossier](Docs/SEL_V2_BACKEND/README.md), [AGENTS.md](AGENTS.md), état actuel du dépôt et PR anciennes pertinentes.

## PRIORITÉ ABSOLUE
Livrer un **backend Rust API complet et indépendant de l'interface**, avec PostgreSQL central, SQLite local **offline-first**, sync idempotent avec conflits, eau/électricité sur facture commune même eau=0 m³, ventes de kits solaires avec contrats/échéances, paiements partiels ordinaires non échelonnés, créances exactes, vérification sécurité, migrations SQL, tests et OpenAPI. **Aucun nouveau frontend** (Tauri/React/desktop refonte) tant que le backend n'est pas validé en production; adapter l'interface existante pour test seulement si indispensable.

## GARANTIE DATA-ONLY SUR LE SERVEUR
**Jamais de PDF/fichier de facture sur le serveur**, même temporairement dans disque, DB `BYTEA`, objet distant, cache, log, export ou backup. Le serveur stocke exclusivement les **données structurées, snapshots, montants, états et références**. La génération/impression/export du PDF se fait **côté client** depuis `GET /v1/invoices/{id}/document-data` (JSON), y compris depuis cache local hors ligne. Interdire une route serveur PDF. Prouver cette propriété par tests.

## MANDAT ET GATES
Tu peux créer une branche `feat/sel-v2-rust-backend`, coder le backend, tester, produire artefacts de développement et ouvrir une **PR non fusionnée**. **Tu ne peux pas** fusionner, publier, migrer une base SEL réelle, déployer sur le VPS réel ni toucher à une release ou installation sur PC sans validation séparée. Le travail doit se faire sous `v2/` en protégeant toutes les PR V1 (notamment #47, #54, #55).

Corrige intelligemment les incohérences de modèle ou d'UX **dans le périmètre métier**, sans inventer de fiscalité, tarifs SEL ou règles de crédit. Fournis une progression **réellement prouvée**, pas un pourcentage imaginé ni une promesse de full production en 23 h. Le rapport final `Docs/SEL_V2_BACKEND/IMPLEMENTATION_REPORT.md` doit contenir SHA, tests, réponses API, statuts offline/online, conservation des données et verdicts GO/NO-GO indépendants.

**Commence maintenant par les ADR et migrations, puis le vertical métier P0 et le protocole de synchronisation. N'implémente aucun nouveau frontend.**
