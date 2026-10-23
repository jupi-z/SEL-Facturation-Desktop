# Plan d'exécution, contrôle continu et rapport de clôture Codex

## Phases à suivre
**Phase 0 — Synchronisation et diagnostic (10 %)**  
Recontrôler PR #47/HEAD, main/CI et PRs #41/#46; lire les specs et vérifier les chemins exacts; produire matrice « constat, cause, test, correctif ». Vérifier la base SQLite uniquement avec copie synthétique. Ne pas supposer que l'ancien audit est toujours valable.

**Phase 1 — Tests rouges et correctifs financiers P0 (30 %)**  
Reproduire conversion CDF incohérente par route HTTP et extraction PDF; fixer ordre sérialisation, conversions et libellés par service; sécuriser sélection de tarifs actifs/dates/devise et profils fiscaux. Tests rouges → verts pour FX-01..09 et TA-01..09. Vérifier l'absence de faux prix et taxe d'eau.

**Phase 2 — Contrat eau universelle et données (20 %)**  
Conserver obligatoirement eau 0 sur tous les nouveaux documents; découpler affichage et raccordement; enlever coercition des données clients; confirmer zéro sans automatisme trompeur; vérifier anciens snapshots, paiements, exports et migrations idempotentes. Tests BI et DATA verts.

**Phase 3 — Documents et non-régressions (15 %)**  
PDF A4 USD/CDF 0/positif, QR interne, libellés, anciennes versions, mise en page. Inspection image/PDF synthétiques; tableau de rapprochement. Non-régression du parcours CRUD et de la protection SQLite.

**Phase 4 — Windows et gates (15 %)**  
Suite complète de tests, compilation EXE/MSI/Burn, CI verte sur SHA final. Si matériel à disposition, recette Windows complète; sinon preuve CI, insuffisance documentée et NO-GO matériel. Contrôle non régression SafeUpdate, maintien fail-closed.

**Phase 5 — Documentation de vérité et PR (10 %)**  
Mettre à jour le rapport de la PR technique et le README, corriger toutes statistiques obsolètes; produire `Docs/SEL_RELEASE_GATE_V1/RAPPORT_EXECUTION_CODEX.md`, liste P0/P1, tests/pass/fail, artefacts et SHA, GO/NO-GO revues distincts. Rebase propre si nécessaire; PR ouverte, jamais fusionnée.

Les pourcentages sont **une grille de reporting**, pas des faits anticipés. Dans chaque phase, annoncer `preuves terminées / preuves requises`, et la progression totale sur **critères réellement satisfaits**. Une phase avec test P0 échoué ne peut pas être à 100 %. La recette matérielle peut rester `BLOQUÉ`; ne pas afficher « livré » dans ce cas.

## Contrôle indépendant à pratiquer
- Avant tout patch : décrire bug et produire au moins un test de régression qui le fait échouer.
- Après patch : suite ciblée; après séries de corrections : suite complète; après rebase final : suite complète + CI au HEAD PR.
- Séparer `bug vérifié`, `hypothèse`, `exigence métier`, `non testé`. Toute anomalie détectée hors cahier mais entraînant corruption, perte de données ou erreur de facture doit être corrigée ou bloquer la livraison.
- Ne jamais remplacer une assertion financière par une recherche de chaîne « USD » ou « CDF » dans PDF. Vérifier exactitude arithmétique et cohérence des unités.
- Pour chaque 0 m³ : prouver la quantité, l'affichage, le montant, le tarif inconnu « — » et aucune TVA supposée si non approuvée.
- Vérifier les données anciennes **avant/après** migration : faire un fingerprint de snapshots, IDs et montants; préserver les anciens soldes et historiques.
- Pas de tests marqués `xfail`/skip pour éviter des échecs P0; si un environnement empêche le test, classer BLOCKED avec cause.

## Contenu obligatoire du rapport final
1. Identifiants branches, base/main, SHAs avant et après, PR corrigée.
2. Tableau `issue → cause racine → fichier/fonction → patch → test rouge/vert → preuve`.
3. Tableau de rapprochement au moins 4 PDF (USD et CDF, eau 0 et eau >0) et détails arithmétiques; 2 exemples de clients historiques synthétiques.
4. État complet tests (nombre, PASS/FAIL/BLOCKED), machine, OS, Python, date, commande, CI URL, paquet EXE/MSI/Burn et hashes (s'ils sont réellement construits).
5. État de migration, sauvegarde/restore, paiements et export portable v1; preuves de conservation des anciens snapshots.
6. Liste explicite de décisions SEL requises : tarifs eau/m³, types, date, devise, assiettes fiscales, éventuels frais fixes et documents.
7. Statuts GO/NO-GO **distincts** : code PR, Windows test, PC SEL, exploitation commerciale et SafeUpdate production.
8. Liste des limites matériel non testé et plan de vérification utilisateur de bout en bout.
9. Liste des PRs obsolètes/non fusionnées et recommandation justifiée (sans les fermer automatiquement).
10. Lien de la PR contenant le travail + demande d'approbation de fusion/release.

## Interdits
Aucun merge sur main, aucune release, aucune modification des données SEL en production, aucune clé privée/secret dans GitHub, aucun contournement de sécurité Windows, aucune invention fiscale et aucune déclaration « 100 % prêt pour SEL » fondée uniquement sur des tests synthétiques.
