# SEL V2 — dossier d'exécution Codex : navigation intégrale, UX et CRUD sûr

**Préparation :** 6 novembre 2026 · **Nature :** documentation d'intégration uniquement.  
**Dépôt :** https://github.com/jupi-z/SEL-Facturation-Desktop  
**Base examinée :** `main@109555533848d441444830748236f74714c2c8fd` (repère **à réactualiser**, pas un SHA à imposer).  
**Cible d'implémentation :** `v2/windows-app` (React / TypeScript / Tauri 2) + seulement les adaptations nécessaires du backend Rust et de ses contrats. **Ne pas commencer une nouvelle application.**

## Démarrage

1. Lire intégralement [CODEX_MASTER_PROMPT.md](CODEX_MASTER_PROMPT.md), puis les documents ci-dessous.
2. Inspecter l'état actuel : branche, HEAD, PR actives, CI, modifications concurrentes, `AGENTS.md`, `README.md`, `v2/README.md` et sources.
3. Ouvrir le fichier **Figma officiel** et relever les composants / frames **réels** (ne pas extrapoler le code à partir d'une capture).
4. Faire un tableau `exigence → Figma frame → UI existante → API → permission → test → écart`.
5. Construire et tester par parcours vertical, en commençant par les P0 et sans dégrader les fonctions existantes.
6. Ouvrir une PR technique **non fusionnée** avec les preuves et le bilan `GO/NO-GO`. Aucun déploiement réel.

## Sources Figma

Fichier : https://www.figma.com/design/EuIpbci6IR7m6ggApo5WWm/SEL-V2-%E2%80%94-Product-Design-%C2%B7-Windows---Facturation

- **Page 16 : « SEL V2 — Prototype intégral & CRUD »** — point d'entrée **H00** : https://www.figma.com/design/EuIpbci6IR7m6ggApo5WWm/SEL-V2?node-id=179-1481
- **Page 17 : « SEL V2 — Règles CRUD & Handoff UX »** — source des décisions, matrice de corrections et recette : https://www.figma.com/design/EuIpbci6IR7m6ggApo5WWm/SEL-V2?node-id=191-20
- **Matrice CRUD Figma :** https://www.figma.com/design/EuIpbci6IR7m6ggApo5WWm/SEL-V2?node-id=191-48
- **Écarts API :** https://www.figma.com/design/EuIpbci6IR7m6ggApo5WWm/SEL-V2?node-id=191-76
- **Recette UX :** https://www.figma.com/design/EuIpbci6IR7m6ggApo5WWm/SEL-V2?node-id=191-103
- Pages 10–15 : composants, 39 précédentes maquettes, états et premier handoff ; pages 00–09 historiques conservées.

**État de conception vérifié lors du handoff :** 76 écrans Figma au format 1440×900, 76 accessibles depuis H00 dans le graphe de prototype, 937 transitions configurées, aucune destination invalide, deux points de départ (H00 et S00). Ce sont des **preuves structurelles Figma**, **pas** une preuve de parcours backend ou de tests utilisateurs. Vérifier à nouveau les frames et les interactions à l'ouverture.

## Dossier (ordre de lecture)

| Document | Objet |
|---|---|
| [CODEX_MASTER_PROMPT.md](CODEX_MASTER_PROMPT.md) | Mission complète à exécuter |
| [01_SOURCES_PRIORITES_ET_GARDES.md](01_SOURCES_PRIORITES_ET_GARDES.md) | Architecture actuelle, conflits historiques et limites |
| [02_NAVIGATION_ET_PARCOURS.md](02_NAVIGATION_ET_PARCOURS.md) | 6 domaines, flux, contexte et comportements UX |
| [03_CRUD_API_ET_RBAC.md](03_CRUD_API_ET_RBAC.md) | Matrice de correction par type d'objet et lacunes API |
| [04_PLAN_IMPLEMENTATION.md](04_PLAN_IMPLEMENTATION.md) | Livraisons verticales, fichiers, exigences P0/P1/P2 |
| [05_RECETTE_ET_PREUVES.md](05_RECETTE_ET_PREUVES.md) | Scénarios, tests automatiques, Windows, critères de sortie |
| [06_MODELE_RAPPORT_FINAL.md](06_MODELE_RAPPORT_FINAL.md) | Rapport d'implémentation à renseigner par Codex |
| [07_PROMPT_COURT_A_COLLER.md](07_PROMPT_COURT_A_COLLER.md) | Prompt de lancement à copier dans Codex |
| [08_CORRECTIF_CATALOGUE_SOLAIRE_SKU.md](08_CORRECTIF_CATALOGUE_SOLAIRE_SKU.md) | Correctif prioritaire : SKU invalide sur la création d'un kit solaire, validation locale et messages d'erreur |

## Protections essentielles

- **Le serveur Rust est l'autorité** de tous les montants, calculs, remises, droits, états financiers, versions, émissions, affectations, contrepassations et soldes. Aucun calcul financier en JS et aucun stockage de PDFs sur le serveur.
- **Facture combinée électricité + eau**, y compris `0 m³` sur la ligne d'eau, PDF généré localement, références et snapshots immuables.
- Le **client Windows actuel est connecté** et **n'effectue pas de facturation métier hors ligne**. Ne pas réintroduire des clients historiques Python, PyWebView, Flask, un agent local SQLite, une outbox ou une promesse offline via cette mission.
- **CRUD ne signifie pas DELETE partout :** annuler un brouillon local ; archiver/désactiver si l'API le prévoit ; corriger un relevé par ajout ; annuler une facture, contrepasser un paiement ou émettre un avoir via écritures traçables. Ne jamais supprimer des écritures financières ou forcer un endpoint non exposé.
- **Code existant** : préserver les composants métier, les API, l'authentification, le SafeUpdate, les tests, les migrations et les documents historiques. Ne pas exécuter de migrations ni d'opérations sur données réelles SEL.
- **Aucune fusion de PR, aucune release, aucune signature fictive, aucune installation ou déploiement de production sans accord distinct.**

## Résultat attendu

Une PR technique unique, revuable, et une implémentation réellement utilisable et vérifiable des parcours P0/P1 compatibles avec l'API existante ; une liste séparée des lacunes backend ; des tests exécutés et un rapport précis des limites. Les capacités sans API doivent être **désactivées et expliquées** tant qu'elles ne sont pas implémentées et testées.

Le présent dossier est une spécification de passation ; **aucune modification du programme n'est effectuée par sa création**.
