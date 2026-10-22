# Codex — mission SEL eau + électricité (22 octobre 2026)

> **Entrée unique pour la prochaine intervention de Codex.** Ce commit est *documentaire uniquement*. Il n'autorise aucune modification de production avant lancement explicite de la mission.

## Commencer ici

1. Lire intégralement [la mission Codex](Docs/SEL_BILLING_COMBINED_V1/00_MISSION_CODEX.md).
2. Traiter [la spécification normative](Docs/SEL_BILLING_COMBINED_V1/01_SPECIFICATION_NORMATIVE.md) comme référence métier/technique.
3. Utiliser [la matrice de tests et de recette](Docs/SEL_BILLING_COMBINED_V1/02_TESTS_ET_RECETTE.md).
4. Respecter [les décisions métier non confirmées par SEL](Docs/SEL_BILLING_COMBINED_V1/03_DECISIONS_A_CONFIRMER_SEL.md).
5. Suivre [la maquette A4 du document combiné](Docs/SEL_BILLING_COMBINED_V1/04_MAQUETTE_A4.md).
6. Consulter le [README du dossier](Docs/SEL_BILLING_COMBINED_V1/README.md), puis les règles préexistantes [AGENTS.md](AGENTS.md).

## Périmètre strict

Créer une **facture/décompte unique eau + électricité**, avec deux sections chiffrées distinctes, un total commun et un **QR documentaire interne SEL** clairement identifié (jamais présenté comme QR fiscal DGI). Les relevés, unités, tarifs et taxes eau restent **inconnus tant que SEL ne les a pas validés**. Ne jamais générer une fausse facture combinée réelle pour compenser ces informations.

Conserver le logiciel Python/Flask/SQLite/ReportLab et son fonctionnement Windows, le modèle de données historique, les factures immuables et l'export `sel-facturation-portable` v1. Les modifications à apporter pendant la mission Codex restent circonscrites au rendu A4 et à l'extension minimale indispensable du parcours/snapshot de facturation. **Pas de nouvelle version commerciale ni de fusion sur `main` sans validation du propriétaire.**

## Procédure Codex

- Vérifier l'état réel de `main` au moment de l'exécution ; base documentaire vérifiée le 22/10/2026 : `baf7a88aae9696b29dff4fd45919daad861c9d7a`.
- Auditer les chemins concernés ; faire un plan minimal et tests de non-régression avant de coder.
- Implémenter sur une branche dédiée ; fournir une PR non fusionnée et des PDF d'essai synthétiques pour revue.
- Rendre compte des tests exacts, des inconnues SEL et des validations manuelles non effectuées.
