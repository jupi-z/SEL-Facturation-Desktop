# Plan de finalisation SEL V2 — backend et frontend Windows

**Objectif :** terminer SEL V2 par des parcours métier utilisables de bout en bout, de l’interface Windows à PostgreSQL, avec usage local hors connexion.

**Référence de planification :** branche monarque/sel-v2-backend, commit b051eb0c48aa7c1b10d8874c3715770195145077, PR #58 ouverte vers main. Ce repère est historique ; vérifier l’état distant au début de chaque reprise.

**Architecture :** Rust est l’unique autorité pour les calculs financiers et les règles métier. SQLite conserve les données locales et l’outbox ; PostgreSQL est la base centrale. Le client Windows reprend Figma dans la pile native existante, sans reproduire la logique financière.

**Portée :** finaliser backend, client Windows, migration, exploitation et recette. L’interface V1 peut être retirée après couverture des parcours V2 et validation de la conservation des outils d’export et migration.

**Design de référence :** fichier Figma « SEL V2 — Product Design · Windows - Facturation », clé EuIpbci6IR7m6ggApo5WWm.

## Contraintes globales

- Les factures historiques ne sont ni recalculées ni réécrites.
- Les identifiants locaux et centraux restent stables pendant la synchronisation.
- Factures, paiements et créances sont transactionnels, idempotents et auditables.
- Le frontend ne calcule aucun montant.
- Une facture de services affiche toujours l’électricité en kWh et l’eau en m³, y compris 0 m³.
- Aucun tarif, impôt, intérêt ou frais n’est inventé ; les paramètres commerciaux doivent être approuvés et historisés.
- Les fichiers PDF restent sur le poste client ; le serveur ne conserve que des données structurées.
- Les ressources statiques nécessaires sont embarquées localement.
- Les mises à jour natives sont protégées par SafeUpdate ; les ressources déclaratives par LiveUpdate.
- Migration ou déploiement réel exige sauvegarde vérifiée et procédure de retour arrière.
- L’autorisation de fusion donnée par le propriétaire s’applique quand les critères de stabilité ci-dessous sont satisfaits.

## Phases et jalons

| Phase | Travail ciblé | Condition de sortie | Difficulté |
|---|---|---|---:|
| 0. Référence et écarts | Vérifier SHA, branche, PR, CI, capacités backend, interface, Figma, artefact livré et outils V1. | Point de retour et matrice des fonctions/écarts établis. | 3/10 |
| 1. Contrats et sécurité | Identités, authentification, rôles, appareils, grants offline, audit, protections API et configuration fail-closed. | Contrats versionnés ; permissions et identités testées. | 8/10 |
| 2. Référentiels | Tarifs et taxes datés/approuvés, devises, catalogue solaire, administration et cache offline. | Paramètres modifiables sans code, versions historisées. | 7/10 |
| 3. Clients et services | Recherche/CRUD client, relevés, facture combinée, aperçu, émission, historique et créance. | Facture eau + électricité utilisable via API locale et centrale. | 6/10 |
| 4. Paiements | Choix des créances, paiements partiels, affectations, soldes, reçus et corrections permises. | Zéro doublon ; parcours disponible dans le client. | 7/10 |
| 5. Offline et convergence | Outbox, worker, ACK, pull, conflits, reprise, deux postes et panne après commit. | Test SQLite A + PostgreSQL + SQLite B reproductible et convergent. | 9/10 |
| 6. Kits solaires | Catalogue, devis, vente comptant, contrats, échéances, paiements, retards et documents. | Une seule obligation financière par vente ; usage offline et sync vérifiés. | 7/10 |
| 7. Frontend Figma | Inventaire des frames, tokens/assets locaux, shell, navigation, écrans et états. | Écrans essentiels reliés aux API et vérifiés visuellement sur Windows. | 7/10 |
| 8. Documents | Données JSON, génération locale des factures/reçus, QR interne, aperçu, export et impression. | PDF A4 et impression vérifiés ; aucun fichier de facture côté serveur. | 7/10 |
| 9. Migration et exploitation | Import V1, rapprochement, sauvegarde/restauration, TLS, secrets, supervision, rollback et mises à jour. | Environnement de test reproductible et restauration prouvée. | 8/10 |
| 10. Recette et retrait V1 | Installation, mise à niveau, tests métier, offline, sync, documents et décision de retrait. | Rapport d’acceptation et décision de stabilisation. | 7/10 |

## Critères de phase

### Phase 0 — référence et écarts

- Relever l’état du worktree et préserver les changements existants.
- Vérifier le SHA distant, l’état de la PR #58, ses contrôles et sa cible.
- Identifier le paquet remis à SEL si les preuves existent ; sinon documenter les éléments manquants.
- Relier chaque écran Figma aux routes, contrats et tests existants.
- Préserver les outils V1 nécessaires à l’export et la migration jusqu’à leur remplacement validé.

### Phases 1 à 6 — backend

- Les montants sont calculés par Rust avec décimaux.
- Une commande locale écrit l’opération, l’audit et l’outbox atomiquement.
- Un ACK signifie que l’effet métier est durablement appliqué dans PostgreSQL.
- Rejeu et conflits sont idempotents, explicites et auditables.
- Eau à zéro et eau positive sont vérifiées avec des tarifs et profils approuvés.
- Les affectations financières sont verrouillées et contrôlées contre les courses.
- Les erreurs métier ont des réponses API documentées.

### Phase 7 — frontend Figma

- Auditer les frames desktop, composants, polices, tokens, variantes et assets.
- Réutiliser composants connectés ou composants existants qui correspondent.
- Embarquer les assets localement ; aucune URL Figma temporaire ne reste dans le code.
- Relier les composants visuels aux contrats Rust sans déplacer les calculs côté client.
- Présenter chargement, vide, erreur, offline, attente, confirmation, conflit et permission refusée.
- Comparer chaque écran intégré aux frames de référence dans WebView2 à dimensions desktop fixées.

### Phase 8 — documents

- Générer les documents à partir des snapshots immuables et des données document-data.
- Préserver le contenu des documents historiques.
- Identifier le QR comme référence interne SEL, sans allégation fiscale.
- Vérifier pagination, polices locales, marges A4, caractères, totaux et lisibilité.

### Phases 9 et 10 — exploitation

- Séparer strictement les environnements de test et de production.
- Tester restauration sur environnement non productif avant toute procédure réelle.
- Simuler la migration, rapprocher les factures/paiements/soldes et rejouer pour vérifier l’idempotence.
- Tester installation, réparation et mise à niveau sans écraser base, identité, outbox ou documents.
- Retirer V1 seulement après couverture V2 et validation des outils de migration/export.
- Marquer BLOCKED_EXTERNAL toute validation nécessitant poste, réseau, secrets ou décision SEL absente.

## Autorisation et critères de fusion dans main

Le propriétaire autorise Codex à fusionner dans main dès que le système est jugé stable et suffisamment fonctionnel. Cette autorisation concerne la fusion du code. Elle n’autorise pas automatiquement un déploiement, une migration ou une écriture financière réelle.

Avant fusion, toutes les conditions suivantes doivent être vérifiées :

1. La PR ne contient que des modifications examinées et aucun secret.
2. La CI pertinente est verte sur le SHA exact : backend Rust, PostgreSQL, SQLite, client Windows, installateur et contrôles de sécurité.
3. Les parcours livrés de facturation, paiement, créance et solaire sont accessibles depuis le client Windows et validés par API.
4. La synchronisation requise pour ces parcours passe avec deux postes, reprise après redémarrage et rejeu sans doublon.
5. Les migrations sont additives et les tests préservent factures, paiements, identifiants et outbox.
6. Le frontend ne calcule pas les montants et aucun fichier facture n’est stocké sur le serveur.
7. Les clés/identités de production sont protégées et séparées des tests.
8. Les limites résiduelles sont documentées et aucun risque P0 de sécurité, intégrité ou perte de données n’est ouvert.

Si un critère échoue, corriger ou consigner le blocage puis réévaluer. Après fusion, vérifier le SHA de main et les contrôles post-fusion. Le déploiement en production conserve des critères distincts.

## Progression

Le registre courant est Docs/SEL_V2_BACKEND/IMPLEMENTATION_REPORT.md. Chaque module reçoit un statut parmi COMPLETED, IN_PROGRESS, PARTIAL, NOT_STARTED, BLOCKED_EXTERNAL, et cinq niveaux séparés : code écrit, tests, utilisable depuis Windows, vérifié sur poste réel, déployé.

Un pourcentage ne compte que les critères de phase ayant une preuve. Une phase n’est pas terminée tant que chacun de ses critères de sortie n’est pas validé.
