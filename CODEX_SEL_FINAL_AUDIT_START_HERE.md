# Codex — mission prioritaire : stabiliser SEL Facturation jusqu'à une PR vérifiable

> **Nouvelle mission** de stabilisation financière et commerciale bi-service. **Lire intégralement** les documents ci-dessous avant toute modification. Cette entrée a été publiée comme **documentation seulement**, sans changement du programme.

## Tu dois travailler sur le vrai état actuel
Dépôt : https://github.com/jupi-z/SEL-Facturation-Desktop  
Issue de suivi : https://github.com/jupi-z/SEL-Facturation-Desktop/issues/49  
PR à finaliser : https://github.com/jupi-z/SEL-Facturation-Desktop/pull/47

Photographie initiale du 23/10/2026 : `main@d37ec5d1`, `PR47@df3be22`, CI Windows #39974626585 réussie. **Revalider ces références** au démarrage; elles peuvent changer.

## Lecture obligatoire dans cet ordre
1. [Diagnostic et contrat normatif](Docs/SEL_FINAL_STABILIZATION_V1/01_DIAGNOSTIC_ET_CONTRAT.md).
2. [Plan et règles d'exécution](Docs/SEL_FINAL_STABILIZATION_V1/02_PLAN_EXECUTION_ET_REGLES_AGENT.md).
3. [Matrice exhaustive de tests/recette](Docs/SEL_FINAL_STABILIZATION_V1/03_MATRICE_TESTS_ACCEPTATION.md).
4. [Rapport et exigences de preuve](Docs/SEL_FINAL_STABILIZATION_V1/04_RAPPORT_PREUVES_ET_GO_NO_GO.md).
5. [README du dossier](Docs/SEL_FINAL_STABILIZATION_V1/README.md), [AGENTS.md](AGENTS.md), [rapports antérieurs](Docs/SEL_EAU_ELECTRICITE_CRUD_RAPPORT.md), [SafeUpdate V1](CODEX_SAFEUPDATE_START_HERE.md), puis issue #49 et PR #47.

## Exigences centrales
- **Toutes les nouvelles factures** : eau (m³) et électricité (kWh) sur le **même document**, **même si eau=0 m³**, avec un total commun. Ne jamais facturer eau positive sans tarif et profil fiscal de SEL **confirmés**.
- **Priorité absolue : corriger les anomalies financières**, notamment l'affichage USD→CDF des **vrais PDF HTTP**, où le code peut mélanger montants USD/CDF et multiplier plusieurs fois une valeur intermédiaire. **Ne pas répéter une conclusion chiffrée non confirmée par le PDF réel.**
- Écarter réellement les tarifs inactifs dans la recherche des barèmes; vérifier les dates, les catégories, les devises et l'état confirmé.
- Séparer affichage de deux lignes obligatoires et statut **physique** du raccordement eau; ne pas inventer que tous les clients sont raccordés physiquement.
- Sécuriser le choix 0 m³ contre une saisie involontaire, préserver les anciens documents, montants et paiements.
- Ne pas modifier arbitrairement le modèle d'application Python/Flask/SQLite/ReportLab, le QR fiscal ou SafeUpdate; pas de contournement SmartScreen ou Ed25519.

## Méthode et fin de mandat
Codex dispose d'une autonomie complète pour **coder, corriger, tester, auditer, compiler et ouvrir/mettre à jour une PR non fusionnée**, en gardant la continuité de la **PR #47**. Il ne doit ni abandonner des tâches techniquement réalisables ni déclarer 100 % sans vérification des critères. Reporter par phase le % réel (`PASS/total`) et les blocages externes.

### STOP-GATE — autorisation supplémentaire impérative
**Ne pas fusionner dans main. Ne pas publier de nouvelle release ni activer un canal de déploiement. Ne pas déployer chez SEL. Ne pas exposer de clé privée ni altérer les bases réelles.** Ces opérations attendent une validation distincte du propriétaire.

La mission se clôture par un rapport `Docs/SEL_FINAL_STABILIZATION_V1/IMPLEMENTATION_REPORT.md` sur la branche technique, avec tests exacts + CI du dernier SHA + preuves PDF/Windows, verdicts séparés **GO technique / GO pilote Windows / GO commercial** ou leur contraire, et liste des validations externes encore nécessaires.
