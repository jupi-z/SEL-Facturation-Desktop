# SEL — Dossier final de stabilisation du moteur commercial eau + électricité

**Date de référence : 23 octobre 2026.**

Ce dossier est **documentaire seulement**. Son intégration dans `main` ne modifie ni le moteur, ni SQLite, ni les installateurs, ni une version publiée.

**Point d'entrée unique Codex :** [CODEX_SEL_FINAL_AUDIT_START_HERE.md](../../CODEX_SEL_FINAL_AUDIT_START_HERE.md)

| Ordre | Document | Objet |
|---|---|---|
| 1 | [Diagnostic et contrat normatif](01_DIAGNOSTIC_ET_CONTRAT.md) | Défaillances démontrées, portée, interdictions et distinction des validations |
| 2 | [Plan d'exécution et règles agent](02_PLAN_EXECUTION_ET_REGLES_AGENT.md) | Reprise correcte PR #47, TDD, progression factuelle et limites de mandat |
| 3 | [Matrice tests et critères d'acceptation](03_MATRICE_TESTS_ACCEPTATION.md) | FX/TR/BI/W, scénario eau 0 et PDF CDF réel |
| 4 | [Rapport de preuves et GO/NO-GO](04_RAPPORT_PREUVES_ET_GO_NO_GO.md) | Format de restitution avec lien CI, logs, captures et blocages externes |
| 5 | [Issue de suivi #49](https://github.com/jupi-z/SEL-Facturation-Desktop/issues/49) | Synthèse suivi, tickets P0/P1 et dépendances |
| 6 | [PR technique #47](https://github.com/jupi-z/SEL-Facturation-Desktop/pull/47) | Travail de Codex à poursuivre, **non fusionné** au moment de l'audit |

## Règles d'arbitrage entre documents du dépôt
- Le cahier actuel est **plus récent** que `CODEX_START_HERE.md` et les spécifications initiales qui autorisaient une facture électricité seule; il prime **uniquement pour le périmètre commercial bi-service et sa stabilisation**.
- Les règles de confidentialité, d'intégrité des données, de facturation DGI et SafeUpdate V1 restent en vigueur sans affaiblissement.
- Aucun choix « tarif eau », « taxe », « remise », « exemption », « statut raccordement » ou « configuration de production » ne peut être fictivement validé au nom de SEL.
- Les anciens rapports `Docs/SEL_EAU_ELECTRICITE_CRUD_RAPPORT.md` sont historiques : Codex doit les réconcilier avec les travaux réellement livrés et les tests actuels.

## Correction importante de l'audit précédent
Il serait inexact d'annoncer sur simple inspection que « le PDF CDF affiche nécessairement deux conversions du total ». Le code courant semble sérialiser `inv.calc_snapshot` **avant** une seconde multiplication dans la variable Python. Le défaut le plus directement suspect est le **mélange de devises et de sous-totaux du PDF**, avec une seconde multiplication inutile. Seul un test HTTP/PDF sur l'artefact réel confirme l'observable. Le contrat `01` et le test `FX-01/FX-07` exigent précisément cette preuve.

## Limitation de livraison
- La PR #47 n'est pas fusionnée; `main` et la release publiée sont antérieurs.
- La CI de #47 réussie ne vaut pas recette de facture CDF, essais impression papier, test matériel SEL ou validation tarifaire de l'eau.
- SafeUpdate est conçu pour des exécutables **non signés Windows**, mais le canal de production demeure **fail-closed** tant que SEL n'a pas établi une clé de confiance Ed25519 propre. Ne jamais désactiver la vérification pour « accélérer » l'installation.
