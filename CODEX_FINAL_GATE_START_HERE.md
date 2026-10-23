# CODEX — SEL Facturation : correction finale eau + électricité

**Point d'entrée unique du mandat de stabilisation du 23 octobre 2026.**

## Démarrage

Lire **entièrement**, dans l'ordre :
1. [Mission et mandat](Docs/SEL_FINAL_RELEASE_GATE/00_MISSION_CODEX.md)
2. [Audit et défauts P0/P1](Docs/SEL_FINAL_RELEASE_GATE/01_AUDIT_ET_ANOMALIES.md)
3. [Contrat fonctionnel normatif](Docs/SEL_FINAL_RELEASE_GATE/02_CONTRAT_METIER_ET_DONNEES.md)
4. [Matrice des tests](Docs/SEL_FINAL_RELEASE_GATE/03_PLAN_DE_TESTS_ET_PREUVES.md)
5. [Phases et progression](Docs/SEL_FINAL_RELEASE_GATE/04_PHASES_PROGRESS_ET_ANTI-REGRESSION.md)
6. [Critères GO/NO-GO](Docs/SEL_FINAL_RELEASE_GATE/05_CRITERES_GO_NO_GO_LIVRAISON.md)
7. [Index](Docs/SEL_FINAL_RELEASE_GATE/README.md) et `AGENTS.md`.

## Travail attendu

**CORRIGER LE CODE**, avec tests rouges→verts, sur la PR technique #47 ou branche corrective dédiée, pas directement sur `main`. Au minimum : double conversion USD/CDF, sélection des tarifs désactivés, cohérence des lignes monétaires, différence entre ligne eau et raccordement physique, risques du zéro prérempli, titres historiques, sécurité migrations, respect des soldes/paiements et tests end-to-end.

**Ne pas réimplémenter ce qui fonctionne déjà.** Contrôler d'abord le HEAD de la PR #47 (base inspectée `df3be22`; elle a une CI Windows verte mais reste ouverte), `main`, PR #41/#46, puis adapter le plan au code réellement présent.

La nouvelle facture comporte **TOUJOURS** `ELECTRICITY` + `WATER`, y compris eau `0 m³`; aucune fausse taxe/prix d'eau. La signature de code Windows n'est pas encore disponible; ne compromettre ni le mécanisme SafeUpdate cryptographique ni les protections de Windows.

**NE PAS** fusionner, publier une release/tag, déployer, introduire une clé privée ou décider les tarifs eau à la place de SEL.

## Fin de mission

La mission Codex doit produire une PR avec `Docs/SEL_FINAL_RELEASE_GATE/IMPLEMENTATION_REPORT.md`, liens CI, PDF de preuve USD/CDF, état de chaque test et verdict **GO/NO-GO distinct pour revue technique, pilote SEL et mise à jour SafeUpdate**. Aucune phase ne peut être annoncée « complète » sans preuve réelle; les validations externes restent identifiées comme bloquées et non inventées.
