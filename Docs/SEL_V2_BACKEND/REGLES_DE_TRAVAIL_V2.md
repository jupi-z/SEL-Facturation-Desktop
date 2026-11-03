# Règles de travail — SEL V2

Ces règles guident les incréments backend, client Windows, migration et exploitation.

## Démarrer un incrément

1. Lire AGENTS.md, le handoff et les documents normatifs du module concerné.
2. Vérifier branche, SHA distant, PR, état du worktree et changements non commités ; préserver le travail existant.
3. Décrire le défaut ou besoin observable et ses critères d’acceptation.
4. Réutiliser d’abord les contrats, routes et composants existants.
5. Choisir une tranche verticale livrable et vérifiable seule.

## Métier et sécurité

- Rust est l’unique autorité des calculs et décisions financières.
- SQLite et PostgreSQL écrivent opérations, audit et outbox dans une transaction.
- Les commandes sont idempotentes ; l’ACK confirme un effet métier durable.
- Les snapshots historiques ne changent pas avec les nouveaux tarifs.
- Aucun tarif, taxe, frais, intérêt ou pénalité n’est inventé.
- Un conflit est visible et auditable ; aucune opération financière n’est silencieusement supprimée.
- Le serveur ne reçoit ni ne conserve de PDF ou image de facture.
- Les identités et clés de production ne sont ni régénérées silencieusement ni commitées.
- Les données SEL réelles ne sont pas utilisées en test ; sauvegarde et rapprochement précèdent toute opération réelle autorisée.

## Frontend

- Respecter les frames Figma, leurs composants, polices, tokens et assets.
- Garder dans le client les données et interactions ; laisser les règles métier à Rust.
- Construire les données dynamiques via DOM sûr, sans concaténation HTML non échappée.
- Prévoir chargement, vide, erreur, hors connexion, attente, confirmation, conflit et refus d’accès.
- Embarquer les ressources nécessaires localement.
- Comparer les écrans desktop aux références Figma dans WebView2.
- Retirer le frontend V1 seulement lorsque les parcours V2 et les outils de migration/export sont couverts.

## Cycle de livraison d’un incrément

1. Écrire les invariants et scénarios d’acceptation.
2. Ajouter ou adapter les tests correspondants.
3. Implémenter le plus petit changement cohérent.
4. Exécuter les validations pertinentes au risque et consigner les résultats exacts.
5. Examiner le diff, migrations, permissions et effets sur les snapshots.
6. Mettre à jour le rapport avec SHA, preuves, limites et validations non réalisées.
7. Créer un commit à sujet unique et le pousser sur la branche de développement.
8. Vérifier la CI sur le SHA exact avant de déclarer validé.

Ne pas supprimer ou désactiver un test pour obtenir une CI verte. Ne pas présenter un test simulé comme une recette réelle.

## Git et fusion

- Branche de développement principale : monarque/sel-v2-backend.
- Préserver PR #58 et tout changement préexistant.
- Le propriétaire autorise la fusion dans main lorsque les critères définis dans PLAN_FINALISATION_BACKEND_FRONTEND.md sont satisfaits.
- La permission de fusion ne permet pas automatiquement un déploiement ni une modification de données SEL.
- Après fusion, vérifier SHA exact de main et les contrôles post-fusion.
- Vérifier les artefacts, sommes et statut test/production avant toute publication.

## Registre et définition de terminé

Le rapport distingue : code présent ; tests exécutés ; parcours utilisable dans Windows ; vérification sur environnement représentatif ; déploiement effectif.

Statuts : COMPLETED, IN_PROGRESS, PARTIAL, NOT_STARTED, BLOCKED_EXTERNAL. Tout COMPLETED cite les preuves. Un incrément n’est terminé que si les critères d’acceptation passent, les validations applicables sont exécutées, le rapport correspond au SHA et les limites restent visibles.
