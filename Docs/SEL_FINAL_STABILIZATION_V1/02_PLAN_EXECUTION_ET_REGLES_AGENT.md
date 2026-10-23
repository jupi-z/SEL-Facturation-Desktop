# Plan d'exécution rigoureux pour Codex — stabilisation SEL

## Méthode sans interruption artificielle

Codex doit mener à terme **toutes les tâches techniquement réalisables avec ses accès**. L'absence de matériel SEL ou de tarifs fiscaux validés est une **dépendance externe documentée**, pas une raison de publier de fausses preuves ni de bloquer les travaux réalisables. Ne pas poser une question déjà résolue dans ces spécifications. Pour les incertitudes métier, préserver les données, garder les paramètres non confirmés et produire des tests avec **fixtures clairement synthétiques**. En fin de mission : PR + rapport exact, y compris tâches externes bloquantes.

### Phase 0 — Revue du dépôt et reprise correcte
- Synchroniser main, PR #47, statut CI, données/règles, version/changes.
- Lire `AGENTS.md`, docs passations, fichiers Python du moteur, PDF, modèles, tests, flux release/packaging et SafeUpdate, issue #49.
- Noter état « déjà fait / absent / incorrect / incertain » pour chaque clause du contrat `01_DIAGNOSTIC_ET_CONTRAT.md`.
- Inventaire final des bugs **reproductibles**, pas de correction à l'aveugle.
- Sélectionner le mode de livraison : commits directement sur la branche #47 si autorisé; autrement branche à partir de la tête de #47 et PR **ciblant la bonne base**. Si branches concurrentes divergent, rebase/merge soigneux avec tests. Ne jamais écraser silencieusement des changements d'un autre agent.

### Phase 1 — P0 financiers
- Écrire d'abord un **test HTTP/PDF échouant** reproduisant incohérence CDF. Vérifier les valeurs extraites d'un PDF réellement créé par `GET /invoices/<id>/pdf?currency=CDF`, non seulement le texte source.
- Corriger une seule fois, préserver snapshots; réconcilier eau/électricité, TVA, encaissements, soldes, sous-totaux et devises.
- Écrire tests tarifs inactifs et sélection temporelle; corriger `db.tariff_plan_for()` et vues dépendantes. Tester la politique de fallback (refus ou ancien tarif, sans réactivation non consentie).
- CI et tests ciblés **verts**.

### Phase 2 — P0 métier + migration
- Garantir deux services obligatoires sur **chaque nouvelle émission**, eau=0 dans PDF/snapshot/export, tarif réel obligatoire pour eau>0.
- Découpler statut raccordement vs règle d'affichage. Ne pas utiliser `UPDATE clients SET water_enabled=1` comme source de vérité physique ; migration additive, tests de non-régression.
- Tester client existant, nouveau, catégories, eau positive et 0, brouillon absent, tarif non confirmé, active=false, devise incompatible, annulation.
- Préserver les données réelles et les snapshots. Migrations ré-exécutables.

### Phase 3 — UX/PDF
- Corriger les risques de 0 prérempli : confirmation à la prévisualisation ou entrée intentionnelle; annulation/re-sélection de client sans perdre des données légitimes.
- Ajuster titre des anciens documents en fonction de leur **contenu snapshot** ; aucune réécriture de factures historiques et aucune fausse ligne d'eau ajoutée à l'historique.
- Vérifier sur PDF synthétique le QR interne, le détail des services et les montants alignés. Prévisualiser et extraire texte PDF automatiquement; si possible exporter des images/aperçus, conserver des artefacts d'audit sans clients réels.
- Tester interactions desktop dans environnement Windows disponible.

### Phase 4 — Contrôle transversal
- Rechercher autres risques qui empêchent vraiment SEL de facturer : correction de paiement, total net, exports, doublons, IDs, contrainte transactionnelle, arrondis, sécurité HTTP locale, migration SQLite, backup/restauration.
- Identifier régressions du projet SafeUpdate; il est acceptable que le canal **production** reste inactif sans clé SEL, jamais de bypass Ed25519.
- Tester `0` eau, `0` électricité, `0` les deux, eau positive avec TVA et taxes confirmées, plusieurs modes de paiement, corrections partielles et intégrales, facture annulée.

### Phase 5 — Windows, CI, livraison de preuves
- `python -m pytest -q` complet; `python -m compileall -q` des modules; `git diff --check`; vérifier que CI Windows **du dernier SHA** passe réellement.
- Construire PyInstaller EXE, WiX MSI, Burn bootstrapper avec WebView2 offline. Vérifier version, taille, sha256, smoke test, démarrage effectif avec GUI si possible. Code signature Windows **non exigée** : documenter `NotSigned` correctement.
- Tester upgrade depuis `v0.3.29` sur une base SQLite représentative, confirmer sauvegarde et reprises. En absence de Windows cible, marquer `NON TESTÉ` au lieu de `PASS`.
- Générer 6 PDF scénarios de recette ; conserver captures / chemins exacts dans rapport sans exposer clients réels.
- Vérifier workflows GitHub `main` + PR #46 (quality gate), mais ne pas fusionner #46 sans instruction du propriétaire.
- Mettre à jour `Docs/SEL_FINAL_STABILIZATION_V1/IMPLEMENTATION_REPORT.md` dans **la PR technique**, avec preuve de chaque critère, et statut final **GO revue technique / NO-GO pilote / NO-GO commercial** distincts.

## Cadence et mesure véridique de progression
Codex doit afficher une progression **par phase**, puis globale, fondée sur les critères testés de la matrice `03_MATRICE_TESTS_ACCEPTATION.md` :
- Tout critère : `NOT_STARTED | IN_PROGRESS | PASS | FAIL | BLOCKED_EXTERNAL | NOT_TESTED`.
- Pourcentage par phase : `PASS / total critères`; les éléments `BLOCKED_EXTERNAL` restent non validés et **ne sont pas comptés comme PASS**.
- Global : `somme PASS / somme critères`; ne pas afficher 100 % si P0 non testé.
- Chaque étape du compte rendu mentionne commits, tests lancés, résultats, erreurs et tâches suivantes; pas de compteurs « inventés ».
- Continuer les travaux malgré un test rouge en corrigeant la cause, sans désactiver le test. Lancer chaque test concerné après correction puis suite totale en fin.
- S'arrêter proprement en présence de données de production exposées ou secrets : ne pas publier ni improviser une migration risquée, signaler le blocage.

## Définition de « terminé »
1. Toutes les tâches réalisables codées et testées, P0 fonctionnels et sécuritaires **PASS** sur environnement d'essai.
2. Rapport exact à jour au dernier SHA, bilan corrigé par rapport aux anciennes PR, PR technique prête à relire, CI verte.
3. Limitations Windows poste SEL, absence de certificat Authenticode, absence de clé SafeUpdate de production et tarif fiscal d'eau non validé **expressément identifiées** et accompagnées d'une procédure de validation.
4. Ne pas fusionner `main` et ne pas publier de release sans validation explicite.

**Attention :** terminer le code n'autorise pas à prétendre que l'application est déjà déployée, fiscalement certifiée ou acceptée par SEL.
