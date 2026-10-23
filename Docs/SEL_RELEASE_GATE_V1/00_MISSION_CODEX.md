# Mission Codex — SEL Release Gate V1 : correction et validation finale

**Date de préparation :** 23 octobre 2026. **Nature de ce commit : documentation uniquement.**
**Source principale :** PR #47 `monarque/water-electricity-crud`, commit vérifié `df3be22e81068b4cbabf78afdcf92fd2cfd8bb87`.
**Base principale à la vérification :** `main@d37ec5d1ec96b4e52c1d60ef0afe28788a6ab804`.
**Contexte concurrent :** PR #41 (ancien périmètre du PDF), PR #46 (quality gate CI), SafeUpdate déjà dans main; ne pas les fusionner automatiquement.

## Mandat explicite
Le propriétaire autorise Codex à **auditer, corriger, tester, construire et ouvrir/mettre à jour une PR technique** pour rendre SEL Facturation utilisable sur le parcours **commercial unique eau + électricité**. L'approche est de stabiliser le code existant, **pas de reconstruire** Flask/SQLite/ReportLab/Windows. Respecter toutes les décisions du propriétaire et le cahier des charges déjà existant lorsque compatible.

**Restrictions incontournables :** ne pas fusionner `main`, ne publier aucune nouvelle release, ne déployer sur aucun poste SEL, ne modifier aucune base réelle, ne créer aucun certificat ou tarif fictif, ne forcer aucune activation SafeUpdate production, **sans approbation explicite ultérieure du propriétaire**. Travailler avec bases anonymes temporaires et artefacts de test.

## Étapes obligatoires (ordre)
1. Lire **tout ce dossier** : 00, 01, 02, 03, 04, 05, README ; puis `AGENTS.md`, `CODEX_START_HERE.md` (historique du cahier eau), `Docs/SEL_BILLING_COMBINED_V1/*`, `CODEX_SAFEUPDATE_START_HERE.md` (contraintes de coexistence), rapport actuel `Docs/SEL_EAU_ELECTRICITE_CRUD_RAPPORT.md`.
2. Actualiser l'état GitHub **au moment de l'intervention** : SHAs main/PR #47, statut CI, nouveaux commits, branches actives et absence de changements concurrents. Un SHA ci-dessus est un point de référence, pas une permission d'écraser des commits ultérieurs.
3. Construire une **matrice de divergence** (exigence → état actuel → preuve/test → réparation → risque). Repérer les régressions de l'ancienne version; la CI verte prouve seulement les tests présents.
4. Travailler **sur la PR #47 si tu en es l'auteur et peux mettre à jour sa branche**, sinon sur une branche dérivée de sa HEAD et une PR de correction dirigée vers cette branche, en veillant à ce qu'une seule PR technique finale contienne réellement toutes les corrections; ne pas produire des PR croisées ambiguës. Conserver les contributions déjà testées.
5. Corriger tous les P0 de `02_EXIGENCES_CORRECTIVES.md`, puis P1, avec tests rouges reproduisant les défauts **avant** patch, tests verts après.
6. Compléter `03_MATRICE_DE_TESTS.md` et la recette Windows de `04_GATE_RELEASE_WINDOWS.md`. Pour les tests machine indisponibles, marquer BLOQUÉ/NON TESTÉ, pas PASS.
7. Remplacer les conclusions historiques/obsolètes du rapport de la PR par un **rapport d'audit final** distinguant ce qui est effectivement implémenté, compilé, testé, installé, imprimé; joindre preuves / captures de documents synthétiques non personnels.
8. Faire un compte rendu final : SHAs, chemins modifiés, tests exécutés, fichiers générés, différences avant/après, risques résiduels, GO/NO-GO par sous-système et GO/NO-GO de livraison; demander validation avant fusion.

## Règles de travail non négociables
- Zéro approximation des montants; `Decimal` et arrondis documentés. Revoir toutes les transformations monétaires et leur réconciliation à l'écran, au PDF, au CSV/JSON, au paiement et dans SQLite.
- Aucune facture nouvelle **électricité seule**; toujours `ELECTRICITY` et `WATER` même pour `0 m³`, sans inventer tarif/fiscalité pour eau non configurée.
- Ne jamais réinterpréter un indicateur de raccordement physique comme une permission d'omettre le service sur le document.
- Ne pas réécrire les snapshots des documents passés, supprimer des paiements, faire de restauration destructive ou changer les numéros historiques.
- Conserver le fonctionnement totalement hors ligne et `%LOCALAPPDATA%\SEL Facturation\sel.sqlite3` ainsi que les sauvegardes et exports.
- QR **référence interne seulement**; aucune usurpation DGI ou faux fiscal. SafeUpdate doit **fail closed** sans clé Ed25519 de production; ne jamais désactiver ce mécanisme.
- Aucune promesse « zéro UAC/SmartScreen » pour un paquet Windows Authenticode non signé.
- Chaque affirmation de test se rattache à la commande, la machine, le résultat et le SHA **réels**.

## Critère de complétion
Codex doit faire **tout le travail autonome réalisable** jusqu'à tests, builds et PR. Il ne doit jamais qualifier la livraison commerciale de GO si restent ouverts : erreur de montant (P0), tarif désactivé utilisé, ancienne facture altérée, perte de données, facturation mono-service sur nouveau document, ou recette Windows réelle non faite. Un blocage extérieur (tarifs/fiscalité SEL, signature/clé production SafeUpdate, poste matériel) est documenté, mais n'autorise pas à fabriquer des preuves.
