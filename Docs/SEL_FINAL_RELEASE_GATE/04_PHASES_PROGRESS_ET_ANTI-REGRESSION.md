# Phases de travail, progression vérifiable et politique anti-régression

## Règle essentielle
Le propriétaire attend un travail **terminé**, pas une annonce de fin parce que le temps ou les outils manquent. Codex travaille jusqu'à clôture de tous les livrables **techniquement réalisables** dans son environnement et signale explicitement les opérations impossibles sans SEL (tarifs, fiscalité, impression sur poste cible, secrets de production). Il **ne simule jamais** ces validations et ne fusionne/publie rien seul.

## Gouvernance de branche / PR

- Reprendre la PR #47 `monarque/water-electricity-crud` ou une branche corrective dérivée de son HEAD, selon conflits et accès. **Ne pas** dupliquer à l'identique les fonctionnalités déjà développées. Ne pas écraser le dernier commit concurrent.
- Inspecter d'abord le dernier `main`, les PR #41, #46 et #47 et les nouvelles CI. La documentation normative sur `main` est le cahier de charges, la PR #47 est le code de travail.
- **Une seule PR finale claire** de correctifs applicatifs vers `main`, avec diff révisable. Si une branche séparée est utilisée, expliquer le lien à #47 et éviter de laisser deux PR dépendantes ambiguës.
- **Interdiction de fusion automatique** (main ou autre), supprimer branches/PR, créer release/tag, publier installateurs, modifier politique GitHub, provisionner clés de production ou toucher la DB SEL. Une demande distincte est nécessaire pour chacune de ces actions.
- Pas d'explosion de périmètre : les fonctionnalités CRUD et SafeUpdate existantes reçoivent uniquement les correctifs de non-régression indispensables. Pas de SaaS, multi-tenant ou développement de nouvelle architecture.

## Phases et pondération de progression

| Phase | Poids | Définition vérifiée de « terminé » |
|---|---:|---|
| 0. État / inventaire | 10 % | SHA + PR + risques + tests reproduisant les anomalies existantes |
| 1. Correctifs calcul / fiscal / FX | 25 % | FX-01 à FX-08, TA-01 à TA-08 et P0 entièrement verts |
| 2. Abonnés / 0 m³ / migrations | 15 % | WA-01 à WA-10 et HI-01/04/06 verts |
| 3. PDFs / historique / paiements / exports | 20 % | PD-01 à PD-05 et HI-02/03/05/07 verts, captures contrôlées |
| 4. Pipeline Windows et régression globale | 15 % | Tests suite complète + compile + diff + Windows CI build verts sur dernier HEAD |
| 5. Rapport, preuves et PR finale | 15 % | Tableau traceabilité complet, issue(s) résiduelles, PR non fusionnée, verdict GO/NO-GO |

**Progression totale = somme des poids uniquement des phases dont les critères sont effectivement satisfaits**. Les étapes partiellement terminées doivent donner leur état d'avancement interne mais le total ne crédite pas une phase P0 incomplète. Les opérations externes non accessibles restent listées comme `BLOCKED_EXTERNAL`, avec `GO PILOTE` interdit; elles ne doivent pas empêcher de livrer un rapport de travail technique achevé.

À chaque phase, mettre à jour le rapport de progression dans la PR ou `Docs/SEL_FINAL_RELEASE_GATE/IMPLEMENTATION_REPORT.md`, avec :
- état phase / total ; pourcentage justifié, pas inventé;
- tests exécutés, nombre PASS/FAIL/SKIP et liens/commands;
- anomalies ouvertes / preuves;
- prochaine action concrète.

## Conditions d'arrêt interdites

Codex NE DOIT PAS :
- déclarer terminé si FX-CDF n'a pas de test assertion numérique de PDF / endpoint réel;
- confondre vérification du rendu PDF avec simple existence d'un fichier PDF;
- déclarer `0 m³` réglé si un ancien client produit encore une facture électrique seule;
- traiter des taxes fictives de fixtures comme données SEL;
- annoncer une release disponible alors qu'elle est seulement brouillon, locale ou artefact CI;
- traiter la CI verte d'un commit antérieur comme celle du code final;
- accepter silencieusement un tarif actif invalide ou un tarif désactivé;
- effacer/réécrire un snapshot de facture émise pour changer sa présentation;
- utiliser tests skip/xfail comme remplacement d'une exigence P0;
- retirer la vérification Ed25519 ou contourner Windows au motif que l'exécutable est non signé.

## Si contradiction ou blocage technique

1. Documenter le comportement actuel et la règle normative.
2. Privilégier le contrat explicite SEL de la facture bi-service et la sécurité des données.
3. Corriger jusqu'au test réussi si les éléments sont déterminés techniquement.
4. Pour les seules décisions nécessitant un document SEL (tarif eau, fiscalité, connectivité), **ne pas inventer** : préparer configuration sécurisée et désactiver émission qui suppose cette donnée.
5. Ne demander arbitrage au propriétaire **qu'une fois** pour les points de décision réellement irréductibles, et livrer toute la portion techniquement réalisable.

## Livrables finaux
La PR doit comporter `IMPLEMENTATION_REPORT.md` avec rubrique « ce qui a changé », « ce qui n'a pas changé », « tests », « PDF rendus », « migrations », « risques », « clés SafeUpdate/Windows », « GO/NO-GO ». Les artefacts de test sont identifiés avec leur provenance/sha et ne sont pas des produits destinés à SEL.
