# Codex — correction finale et vérification de SEL Facturation

> **Mission prioritaire de stabilisation, 23 octobre 2026.** Ce point d'entrée remplace toute simple demande de retouches visuelles pour la **phase finale** de la PR de facturation combinée. Il ne remplace pas les contraintes SafeUpdate ni les décisions métier déjà validées.

**Dépôt** : `jupi-z/SEL-Facturation-Desktop`  
**PR technique principale à consolider** : [#47](https://github.com/jupi-z/SEL-Facturation-Desktop/pull/47), branche `monarque/water-electricity-crud`.  
**Documentation normative exhaustive** : [Docs/SEL_RELEASE_GATE_V1/README.md](Docs/SEL_RELEASE_GATE_V1/README.md).

Lis, **intégralement et dans l'ordre** :
1. [Mission et conditions](Docs/SEL_RELEASE_GATE_V1/00_MISSION_CODEX.md)
2. [Constats réels au dernier audit](Docs/SEL_RELEASE_GATE_V1/01_BASELINE_AUDIT_VERIFIE.md)
3. [Exigences P0/P1](Docs/SEL_RELEASE_GATE_V1/02_EXIGENCES_CORRECTIVES.md)
4. [Tests de preuve](Docs/SEL_RELEASE_GATE_V1/03_MATRICE_DE_TESTS.md)
5. [Gates de livraison Windows](Docs/SEL_RELEASE_GATE_V1/04_GATE_RELEASE_WINDOWS.md)
6. [Phases, critères de complétion et rapport](Docs/SEL_RELEASE_GATE_V1/05_PLAN_CONTROLE_ET_RAPPORT.md)
7. [Index du dossier](Docs/SEL_RELEASE_GATE_V1/README.md)
8. `AGENTS.md`, les documents antérieurs pertinents et le code réel.

## Instructions immédiates
1. Vérifie les commits et l'état du dépôt GitHub à l'instant de démarrage : ne suppose pas que la PR #47 est restée identique.
2. Utilise la PR #47 comme base, **pas `main` seul**, puisqu'elle porte les changements eau + électricité; réconcilie proprement les éventuels nouveaux commits main.
3. Reproduis les anomalies financières et SQL par des tests, puis corrige-les jusqu'à preuve de résolution.
4. Respecte la facture **toujours combinée**, le `0 m³` visible, les tarifs confirmés, les documents historiques, les paiements et la sécurité SQLite.
5. Vérifie le PDF CDF réel par route HTTP, pas une conversion supposée. Le code présente un état intermédiaire doublement multiplié **qui n'est pas nécessairement sérialisé**; la vraie incohérence à démontrer est le mix devises/montants/total.
6. Conduis les phases, tests, CI Windows et compte-rendu jusqu'aux critères de fin. Reporte distinctement les étapes bloquées par le matériel, SEL ou la clé de production.
7. Mets à jour la PR technique et joins `Docs/SEL_RELEASE_GATE_V1/RAPPORT_EXECUTION_CODEX.md`.

**Mandat** : modifier le code **seulement sur ta branche/PR technique**, corriger, tester, compiler et fournir preuves. **Interdit** de fusionner dans `main`, de publier une release, d'écraser une DB SEL ou de contourner SmartScreen/UAC ou l'authentification Ed25519 sans nouvelle validation du propriétaire.

**Définition du succès** : P0 résolus + tests verts + CI Windows au SHA exact + rapports vérifiables + PR prête à la revue. **Ce n'est pas la même chose qu'un GO de livraison PC SEL**. Demande validation avant toute fusion/distribution.
