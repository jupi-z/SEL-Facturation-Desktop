# SEL Final Release Gate — mission Codex de stabilisation

**23 octobre 2026 — dossier DOCUMENTAIRE seulement.**
**Entrée racine : [CODEX_FINAL_GATE_START_HERE.md](../../CODEX_FINAL_GATE_START_HERE.md).**

L'objectif est de faire corriger **jusqu'à complétude démontrable** les anomalies identifiées dans la PR #47 (facture unique eau+électricité, tarif, devise, historique, PDF, paiements, Windows), sans modifier le code par cette publication documentaire.

Lire dans cet ordre :
1. [00 — Mission Codex](00_MISSION_CODEX.md) : mandat, périmètre, contraintes
2. [01 — Audit et anomalies](01_AUDIT_ET_ANOMALIES.md) : reproduction P0/P1, références du code
3. [02 — Contrat métier/données](02_CONTRAT_METIER_ET_DONNEES.md) : invariants bi-service, fiscalité, snapshot
4. [03 — Plan de tests/preuves](03_PLAN_DE_TESTS_ET_PREUVES.md) : scénarios vérifiables
5. [04 — Phases, progrès, anti-régression](04_PHASES_PROGRESS_ET_ANTI-REGRESSION.md) : progression sans auto-satisfaction
6. [05 — GO/NO-GO](05_CRITERES_GO_NO_GO_LIVRAISON.md) : revue PR, pilote, SafeUpdate distincts

Références déjà présentes (à consulter sans appliquer aveuglément les exigences devenues obsolètes) :
- [Documentation facture 2026-10-22](../SEL_BILLING_COMBINED_V1/README.md)
- [Mission SafeUpdate V1](../SEL_SAFEUPDATE_V1/README.md)
- [Instructions des agents](../../AGENTS.md)

**Les exigences d'octobre 23 (eau visible pour tous, même 0) prévalent sur les anciennes sections indiquant « eau seulement si raccordée ».** Pour les informations factuelles sur le raccordement, ne pas falsifier les dossiers des clients.

Cette documentation **n'autorise pas** la fusion de la PR #47, la publication d'une release ou l'installation chez SEL.
