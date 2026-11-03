# Plan de finalisation SEL V2 — backend et frontend Windows

**Objectif :** terminer SEL V2 par des parcours métier utilisables de bout en bout, de l’interface Windows à PostgreSQL, avec usage local hors connexion.

**Référence de planification :** branche monarque/sel-v2-backend, commit b051eb0c48aa7c1b10d8874c3715770195145077, PR #58 ouverte vers main. Ce repère est historique ; vérifier l’état distant au début de chaque reprise.

**Architecture :** Rust est l’unique autorité pour les calculs financiers et les règles métier. SQLite conserve les données locales et l’outbox ; PostgreSQL est la base centrale. Le nouveau client Windows sera reconstruit à partir du fichier Figma, sans réutiliser l’ancien frontend ni reproduire la logique financière.

**Portée :** finaliser backend, nouveau client Windows, migration, exploitation et recette. Le frontend historique est hors cible et peut être abandonné ; ses contrats/API et outils V1 ne seront conservés que s’ils sont nécessaires à la migration ou à l’export des données.

**Directive applicable :** le mandat propriétaire du 3 novembre 2026 autorisant le nouveau frontend Windows V2 et l’abandon de l’ancienne UI remplace, pour ce seul périmètre, l’interdiction de frontend figurant dans le mandat initial `CODEX_SEL_V2_BACKEND_START_HERE.md`. Les protections de V1, la conservation des données et l’interdiction de modifier les données réelles restent en vigueur.

**Design de référence :** fichier Figma « SEL V2 — Product Design · Windows - Facturation », clé EuIpbci6IR7m6ggApo5WWm.

## Relevé Figma — 3 novembre 2026

Le fichier a été ouvert via le connecteur Figma et ses dix pages principales ont été inventoriées : Vision, Fondations, Composants, Parcours, Opérations Windows, Solaire/finance, Administration/système, Documents A4, Spécification de livraison et États/accessibilité. Les vues examinées visuellement comprennent le tableau de bord, la prévisualisation de facture et le spécimen A4. Ce relevé est une base de planification, pas une validation pixel par pixel de chaque écran.

- La page de livraison annonce **34 livrables (30 écrans métier + 4 documents A4)**. Un addendum daté du 3 novembre ajoute **10 écrans Windows et un cinquième modèle A4**. Les frames présents dans les pages de conception incluent aussi plusieurs variantes et écrans V2 additionnels ; le décompte final devra être réconcilié avec la page de livraison avant d’en faire une liste d’acceptation exhaustive.
- Les familles visibles sont : accès/enrôlement ; tableau de bord ; abonnés et dossier client ; facture combinée, aperçu et historique ; relevés eau/électricité ; paiements, créances, situation client, caisse et corrections ; catalogue/vente solaire, comptant, contrats et échéanciers ; utilisateurs/rôles, appareils/grants, tarifs/taxes, devises, audit, synchronisation/conflits, documents/impression, mises à jour, sauvegardes/diagnostics et identité SEL.
- Les livrables A4 couvrent facture combinée acceptée, facture provisoire locale, vente solaire, reçu de paiement et, dans l’addendum, facture combinée avec consommation d’eau positive. Tous les contenus d’exemple sont explicitement fictifs ; les données légales, tarifs, fiscalité et taux restent à confirmer par SEL.
- Direction visuelle : fenêtre desktop 1440×900, navigation latérale bleu nuit `#012A5E`, jaune `#FDDC02` en accent réduit, bleu d’action `#075BBB`, fond `#F7F9FC`, surfaces secondaires, séparateurs fins, typographie Geist, densité maîtrisée et peu d’ombres. Les états ne reposent pas uniquement sur la couleur ; clavier, focus visible et contraste sont spécifiés.
- Parcours prioritaires : facture combinée (client → relevés → tarifs approuvés → calcul Rust → aperçu A4 → confirmation d’émission locale → règlement éventuel → impression locale → synchronisation → accusé) ; solaire (catalogue → client → vente → contrat/échéancier éventuel → règlement/affectation → solde → rapprochement) ; et cycle d’une commande offline avec états `LOCAL_PENDING`, `CENTRAL_ACCEPTED`, `CONFLICT`, `REJECTED`.
- La spécification Figma recense des dépendances API déjà ciblées et des lacunes à vérifier : relevés de compteurs, journal de caisse, reporting, résolution de conflit, administration RBAC, audit, opérations de sauvegarde/diagnostic. Les routes indiquées dans les maquettes ne sont pas présumées implémentées ou conformes : chaque contrat sera confronté au code Rust et aux tests.

## Décision de cadrage du nouveau frontend

Le frontend existant ne sera pas porté ni utilisé comme base visuelle. Il pourra seulement servir de référence transitoire pour comprendre les contrats API locaux et le démarrage de l’agent Rust. Aucun template, écran ou logique de navigation existante n’est une dépendance du nouveau client.

**Architecture candidate à valider en phase 0 :** client React + TypeScript construit en ressources locales, rendu dans une enveloppe Windows native Tauri 2 ; l’enveloppe démarre et supervise `sel-local`, puis le client dialogue avec l’API locale Rust. Le client ne contacte jamais PostgreSQL directement et ne calcule aucun montant. Les versions de Figma fournies en code sont des références structurelles/visuelles, pas du code de production à copier tel quel : les layouts seront reconstruits avec des composants responsives et accessibles. Avant d’engager ce choix, un spike doit valider le pont local, le démarrage/arrêt de l’agent, le packaging WebView2 et l’installateur autonome sur Windows. Si ce spike échoue, le shell natif sera choisi sur la base du risque de déploiement le plus faible, sans reprendre l’ancien frontend.

**Contrat d’intégration visé :** UI → adaptateur de transport local typé/versionné → agent Rust local (SQLite + outbox + worker) → API centrale Rust/TLS → PostgreSQL. L’UI lit des projections, soumet des commandes métier et restitue les états renvoyés ; le moteur Rust demeure seul responsable des montants, permissions et transitions métier. Le rendu A4 et l’impression sont locaux et alimentés par des données structurées versionnées.

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
| 0. Référence et écarts | Vérifier SHA, branche, PR, CI, capacités backend, artefact livré et outils de migration ; réconcilier toutes les frames Figma avec les écrans, API, droits et tests ; valider le spike Tauri/WebView2/agent local. | Point de retour, inventaire Figma exhaustif, décisions d’architecture et matrice écran→API→test établis. | 4/10 |
| 1. Contrats et sécurité | Identités, authentification, rôles, appareils, grants offline, audit, protections API et configuration fail-closed. | Contrats versionnés ; permissions et identités testées. | 8/10 |
| 2. Référentiels | Tarifs et taxes datés/approuvés, devises, catalogue solaire, administration et cache offline. | Paramètres modifiables sans code, versions historisées. | 7/10 |
| 3. Clients et services | Recherche/CRUD client, relevés, facture combinée, aperçu, émission, historique et créance. | Facture eau + électricité utilisable via API locale et centrale. | 6/10 |
| 4. Paiements | Choix des créances, paiements partiels, affectations, soldes, reçus et corrections permises. | Zéro doublon ; parcours disponible dans le client. | 7/10 |
| 5. Offline et convergence | Outbox, worker, ACK, pull, conflits, reprise, deux postes et panne après commit. | Test SQLite A + PostgreSQL + SQLite B reproductible et convergent. | 9/10 |
| 6. Kits solaires | Catalogue, devis, vente comptant, contrats, échéances, paiements, retards et documents. | Une seule obligation financière par vente ; usage offline et sync vérifiés. | 7/10 |
| 7. Nouveau frontend Figma | Construire une application neuve (aucun portage du frontend historique), système de composants/tokens, navigation, écrans et états d’après les frames Figma ; raccorder par un adaptateur typé au service Rust local. | Tous les écrans retenus de l’inventaire réconcilié sont fonctionnels, accessibles, reliés aux API et vérifiés visuellement sur Windows. | 8/10 |
| 8. Documents | Données JSON, génération locale des factures/reçus, QR interne, aperçu, export et impression. | PDF A4 et impression vérifiés ; aucun fichier de facture côté serveur. | 7/10 |
| 9. Migration et exploitation | Import V1, rapprochement, sauvegarde/restauration, TLS, secrets, supervision, rollback et mises à jour. | Environnement de test reproductible et restauration prouvée. | 8/10 |
| 10. Recette et retrait V1 | Installation, mise à niveau, tests métier, offline, sync, documents et décision de retrait. | Rapport d’acceptation et décision de stabilisation. | 7/10 |

## Critères de phase

### Phase 0 — référence et écarts

- Relever l’état du worktree et préserver les changements existants.
- Vérifier le SHA distant, l’état de la PR #58, ses contrôles et sa cible.
- Identifier le paquet remis à SEL si les preuves existent ; sinon documenter les éléments manquants.
- Inventorier les pages, frames, composants, variantes, assets et modèles documentaires Figma ; résoudre le décalage entre l’inventaire de livraison et l’addendum.
- Relier chaque écran Figma aux routes Rust, droits, états, tests et dépendances métier ; marquer les API absentes ou seulement maquetées.
- Tester le démarrage de l’agent local, le transport UI↔agent et les dépendances Windows du shell retenu.
- Préserver uniquement les outils V1 nécessaires à l’export et la migration jusqu’à leur remplacement validé ; le frontend V1 n’est pas une base de réutilisation.

### Phases 1 à 6 — backend

- Les montants sont calculés par Rust avec décimaux.
- Une commande locale écrit l’opération, l’audit et l’outbox atomiquement.
- Un ACK signifie que l’effet métier est durablement appliqué dans PostgreSQL.
- Rejeu et conflits sont idempotents, explicites et auditables.
- Eau à zéro et eau positive sont vérifiées avec des tarifs et profils approuvés.
- Les affectations financières sont verrouillées et contrôlées contre les courses.
- Les erreurs métier ont des réponses API documentées.

### Phase 7 — reconstruction du frontend Figma

- Construire un client neuf, séparé du frontend historique, à partir de l’inventaire approuvé en phase 0.
- Créer localement la bibliothèque de composants, tokens, navigation et formulaires inspirés des frames Figma ; réutiliser seulement les assets de marque autorisés, pas l’ancienne UI.
- Embarquer polices, icônes et images localement ; aucune URL Figma temporaire ni dépendance de fonctionnement à Internet ne reste dans le paquet.
- Implémenter un adaptateur API typé vers `sel-local`; le client ne parle ni à PostgreSQL ni à un moteur métier parallèle.
- Relier les écrans aux contrats Rust sans déplacer les calculs, permissions ou transitions financières côté client.
- Présenter chargement, vide, erreur, offline, attente, confirmation, conflit et permission refusée.
- Comparer chaque écran intégré aux frames desktop de référence à dimensions fixées, puis vérifier navigation clavier, mise à l’échelle Windows et états réseau.
- Garder les données de démonstration clairement identifiées et ne jamais les afficher comme données réelles SEL.

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
