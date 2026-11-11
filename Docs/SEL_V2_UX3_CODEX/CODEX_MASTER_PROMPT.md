# Mission Codex — SEL Facturation V2 : intégrer entièrement le nouveau parcours UX + CRUD

> **Mandat de développement.** Tu es autorisé à analyser, planifier, modifier le code, exécuter les tests, construire les artefacts de validation disponibles et ouvrir une PR de code. Tu n'es **pas** autorisé à fusionner la PR, publier une release, migrer des données SEL réelles ou déployer en production sans validation distincte. Ne modifie le fichier Figma que si indispensable et avec protection des frames sources ; la source de la vérité visuelle est déjà dans Figma.

## 0. Reprise obligatoire et prévention des conflits

- Lire ce dossier dans l'ordre du README, `AGENTS.md`, `README.md`, `v2/README.md`, l'état actuel de `v2/windows-app`, le contrat `v2/openapi/sel-v2.yaml`, les routes `v2/crates/sel-api/src/lib.rs`, la passerelle `v2/windows-app/src-tauri/src/lib.rs` et les instructions de CI.
- Identifier le HEAD réel, la branche de travail, le contenu des PR actives et les changements déjà présents. **Ne pas écraser le travail concurrent.** La base documentaire examinée était `main@109555533848d441444830748236f74714c2c8fd`; vérifier tout nouvel état.
- Créer une **branche de code distincte** de la branche de handoff, dérivée de la base vérifiée. Ne pas écrire directement sur `main`. Si la PR du handoff n'est pas fusionnée, inclure ce dossier de manière sûre, sans perdre les modifications existantes.
- Respecter l'architecture **actuelle** : React + TypeScript + Tauri 2 pour l'application Windows, API Rust centrale + PostgreSQL, client connecté. Les anciens mandats offline/Flask ne constituent pas la cible actuelle. Les clauses encore applicables (préservation V1 et données, immutabilité, sécurité, absence de PDF serveur) restent obligatoires.

**Correctif prioritaire ajouté le 11 novembre 2026 :** lire et intégrer [08_CORRECTIF_CATALOGUE_SOLAIRE_SKU.md](08_CORRECTIF_CATALOGUE_SOLAIRE_SKU.md). L'ajout d'un kit solaire échoue lorsque le SKU contient des espaces ou une virgule ; implémenter une validation locale explicite, préserver la saisie sur erreur, améliorer les messages et tester le flux DRAFT sans modifier les règles Rust. Vérifier la branche technique/PR #60 et son HEAD avant toute modification, sans écraser un correctif éventuellement déjà présent.

## 1. Objectif produit

Livrer l'UX complète SEL V2, simple et sûre, sur la base de **76 écrans Figma** (page 16) et du handoff de **4 planches** (page 17). Le rendu Figma est une référence de composition et d'interaction, pas une injonction de coder 76 composants React indépendants : factoriser en layouts, pages, panneaux, formulaires, détails, dialogs et états réutilisables. Il faut aboutir à des **parcours métiers fonctionnels réels**, pas uniquement des captures ou une navigation décorative.

Figma officiel : https://www.figma.com/design/EuIpbci6IR7m6ggApo5WWm/SEL-V2-%E2%80%94-Product-Design-%C2%B7-Windows---Facturation
Point de départ : https://www.figma.com/design/EuIpbci6IR7m6ggApo5WWm/SEL-V2?node-id=179-1481
Matrice CRUD : https://www.figma.com/design/EuIpbci6IR7m6ggApo5WWm/SEL-V2?node-id=191-48

## 2. Règles de conception

1. Navigation principale **Accueil / Abonnés / Facturation / Paiements / Solaire / Administration** ; diagnostic serveur accessible secondairement. L'administration doit respecter les rôles réels. Marquer la rubrique active et prévoir retour, annulation, chemin du dossier, rechargement, états vides/chargement/erreur.
2. Depuis un dossier, conserver `customerId`, les filtres, le contexte, les valeurs de formulaire et le point de retour lors du passage à facture, relevé ou encaissement. Aucun lien factice, aucun écran orphelin.
3. Toute écriture sensible suit **saisie → vérification serveur → confirmation explicite → résultat**, sauf actions de lecture. Le premier clic sur une ligne ne doit jamais muter une donnée.
4. Séparer création, consultation, modification, désactivation/archivage, annulation/contrepassation, historique et droits. Un bouton « Supprimer définitivement » **n'est pas** la solution universelle.
5. Réutiliser les composants SEL, le vrai logo SVG sans plaque blanche, les variables/couleurs/typographie Geist, les layouts flat, l'accessibilité clavier et les contrastes ; ne pas introduire de captures Figma raster comme interface.
6. Gérer `401/403/409/422/503`, échec réseau, attente, double clic, rejet d'une écriture, demande déjà appliquée (idempotence), conflits `If-Match/ETag` et différence entre retour du serveur et action effectivement validée.
7. Ne pas déclarer un écran terminé si son bouton ne fait rien, n'a pas d'état d'attente ou appelle une route non autorisée.

## 3. CRUD sûr par domaine

- **Abonnés** : liste / recherche / créer / détail / modifier / archiver / restaurer via POST/PATCH serveur (`status=ARCHIVED/ACTIVE`), confirmation et révision `If-Match`. Aucune suppression définitive.
- **Services** : créer, lire, modifier `connection_status`, `contract_ref`, `activated_at`, respecter `If-Match`. Ne pas supprimer des relevés.
- **Compteurs** : consulter et créer ; **pas d'API de modification/remplacement/suppression identifiée**. Afficher une action bloquée plutôt que simuler une opération ; proposer une spécification backend distincte.
- **Relevés** : saisie et historique ; correction append-only via `/v1/meter-readings/{id}/corrections`, avec ancien index conservé, auteur/motif et contrôle de rôle.
- **Facturation** : deux lignes électricité et eau même à 0 m³, profil fiscal/tarifs actifs, prévisualisation Rust, émission seulement après confirmation, PDF local, historique et annulation auditée via `/v1/invoices/{id}/void` sous autorisations.
- **Paiements** : journal, enregistrement, affectations multiples, pièces de reçu, contrepassation (dont partielle si le serveur la permet) avec montant/motif via `/v1/payments/{id}/reverse`, sans supprimer l'écriture originale.
- **Créances** : lecture des soldes du serveur ; émission d'un avoir via `/v1/receivables/{id}/credit`. Pas de remplacement de solde côté React.
- **Solaire** : catalogue approuvé, devis, vente, créance, versements, échéancier **optionnel** ; ne pas imposer d'échéancier aux ventes au comptant. Les corrections de vente et de contrat actif **n'ont pas de routes confirmées** : produire une proposition API sûre, ne pas afficher un faux « Supprimer ».
- **Administration** : création utilisateur, rôle/statut, désactivation et audit ; versions DRAFT/APPROVED/REVOKED pour produits/tarifs/taxes/taux ; création de nouvelle version, comparaison et approbation/révocation explicites, jamais réécriture d'une version historique.

La **matrice précise et les endpoints** sont dans `03_CRUD_API_ET_RBAC.md`. Les routes Rust, le contrat OpenAPI et les permissions priment sur un texte d'interface si divergence.

## 4. Méthode d'implémentation (exécuter, pas seulement proposer)

**Phase 0** — Audit `HEAD`, branches/PR/CI, inventaire des vues actuelles, diff Figma/code/API, tests existants, échantillon de captures. Donner des preuves plutôt que repartir de zéro.

**Phase 1** — Shell de navigation et composants : 6 domaines, sous-parcours, `RouteState` ou équivalent typed, retour/context preservation, breadcrumb, modèle de dialogue, erreurs/chargement/confirmations, tokens SEL, navigation au clavier. Conserver l'identité/offline locks/permissions.

**Phase 2 (P0)** — Parcours facturation combinée, paiements et corrections, clients et services : formulaires éditables, validation serveur avant mutations, états CRUD autorisés, pagination/filtres/détails. Respecter Rust Decimal, idempotence, ETag/If-Match.

**Phase 3 (P1)** — Relevés et correction, solaire et échéances optionnelles, référentiels et utilisateurs, revue/audit, états d'erreur, documents client.

**Phase 4** — Résoudre uniquement les écarts API strictement requis et justifiés par une spécification, tests et sécurité. Pour les corrections absentes (compteurs, ventes solaires, contrats actifs), séparer ce qui est implémenté de ce qui est bloqué : n'inventer aucun endpoint, ne contourner aucun RBAC. Proposer un ADR/contrat et le besoin métier avant d'exposer une commande destructive.

**Phase 5** — Tests unitaires, API/Rust, intégration et recette Windows sur poste adapté si possible, captures comparatives 1440×900 et 1280×720, PDF corrects, CI, PR code et rapport d'implémentation.

Lire les livraisons, critères et commandes dans `04_PLAN_IMPLEMENTATION.md` et `05_RECETTE_ET_PREUVES.md`.

## 5. Sécurité, fiscalité et données

- Aucun montant financier, solde, arrondi, TVA, taux de change ou date d'effet décidé dans le frontend. Toutes valeurs viennent de versions actives approuvées et du moteur Rust. Le cadrage initial évoque 16 % de TVA et 1,30 USD/m³ comme configuration initiale, **non** comme permission d'injecter des constantes commerciales sans validation SEL.
- Facture unique eau + électricité. Quantité eau = zéro explicitement visible ; QR interne n'est pas un QR fiscal DGI.
- Pas de stockage de PDF, images ou fichiers facture sur le serveur ; génération et impression **côté poste Windows** à partir des données structurées confirmées. Ne pas écrire dans une base réelle ou conserver des informations personnelles en tests.
- Préserver les numéros, snapshots, reçus, écritures, révisions, traitements idempotents, refus d'accès, chiffrement des secrets et mécanismes de distribution sécurisés.
- Le client cible **requiert Internet et le serveur central**. Ne réactive pas de logique métier hors connexion sans décision d'architecture séparée.

## 6. Contrat de livraison

Dans la PR technique non fusionnée, fournir :
1. Liste des parcours implémentés avec l'écran Figma/route/API réelle, statut **DONE / PARTIAL / BLOCKED** et critères vérifiés.
2. Tests ajoutés et exécutés ; commandes exactes, statut, OS/machine, HEAD, CI et limites non testées.
3. Comparaisons d'interface, navigation, clavier, erreurs, CRUD, factures, paiements et PDF synthétiques.
4. Preuves des règles de sécurité et de non-régression, aucun nouveau faux endpoint, aucune migration destructrice.
5. Lacunes d'API documentées et propositions de résolution séparées pour compteur/vente solaire/contrat.
6. Rapport `06_MODELE_RAPPORT_FINAL.md` complété, liens vers PR / commit / logs et verdict **GO/NO-GO** pour code, API, UX, Windows, distribution.

**Interdiction de prétendre « 100 % livré » sans recette réelle.** Si un outil, un accès Figma, un serveur, une CI, un runtime Windows ou des données de test contrôlées manquent, terminer tout le travail autonome réalisable et déclarer explicitement `BLOCKED_EXTERNAL` pour l'étape concernée ; ne pas inventer une réussite. **Ne pas demander de reconfirmation d'informations déjà documentées.**

**Commence immédiatement par l'audit de l'état distant et l'inventaire des divergences, puis implémente les phases et livre une seule PR technique documentée.**
