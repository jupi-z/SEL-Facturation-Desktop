# SEL Release Gate V1 — corrections finales demandées à Codex

**Dossier documentaire daté du 23 octobre 2026.** Les fichiers de cette passation ne corrigent aucun code et ne publient aucune application. Ils sont les critères de mission pour corriger et stabiliser la PR #47.

**Commencer par :** [CODEX_RELEASE_GATE_START_HERE.md](../../CODEX_RELEASE_GATE_START_HERE.md).

## Ordre de lecture
1. [00 — Mission Codex](00_MISSION_CODEX.md).
2. [01 — Baseline et anomalies vérifiées](01_BASELINE_AUDIT_VERIFIE.md).
3. [02 — Exigences correctives normatives](02_EXIGENCES_CORRECTIVES.md).
4. [03 — Matrice de tests](03_MATRICE_DE_TESTS.md).
5. [04 — Gates Windows et livraison](04_GATE_RELEASE_WINDOWS.md).
6. [05 — Plan de contrôle et rapport final](05_PLAN_CONTROLE_ET_RAPPORT.md).

## Contrat métier SEL
- Chaque **nouvelle facture** est un document commun comportant **électricité + eau** : même si `0 kWh` ou `0 m³`.
- L'eau se vend au **mètre cube**; eau positive nécessite tarifs et fiscalité **fournis et confirmés par SEL**, jamais inventés.
- Eau 0 sans paramètres eau : ligne à 0 et montant nul, sans qualifier comme exonération ni donner un faux prix unitaire ou taux fiscal.
- Anciens documents, paiements, références, exports JSON et SQLite doivent rester intacts.
- Réparer les PDF CDF, la sélection de tarifs inactifs, la confusion entre « service sur facture » et « raccordement réel ».
- **Ne pas fusionner ni publier** sans approbation expresse.

## Important
Le rapport `Docs/SEL_EAU_ELECTRICITE_CRUD_RAPPORT.md` correspond à plusieurs étapes de développement; il peut mentionner un statut obsolète. Codex doit le mettre à jour dans sa PR pour décrire le **dernier état réellement testé**. Le mécanisme SafeUpdate production ne devient pas fonctionnel par la seule résolution des bugs de facturation, en l'absence de clé Ed25519 de confiance.
