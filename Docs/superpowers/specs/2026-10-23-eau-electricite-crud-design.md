# Conception — Vente combinée d’eau et d’électricité, CRUD sûr

## Intention

Permettre à SEL de gérer les deux services dans un parcours local cohérent et d’émettre pour chaque abonné un document unique présentant les deux consommations et leurs montants. Compléter les opérations de création, consultation, modification et suppression des données de référence sans rendre possible l’effacement silencieux de l’historique financier.

## État de départ constaté

- La pile est Python/Flask/SQLite, application de bureau locale. Les factures contiennent des snapshots JSON immuables; l’export portable v1 existe.
- Le moteur et le formulaire ne calculent que l’électricité. Le PDF version 2 porte déjà un titre eau + électricité, mais n’affiche encore que les lignes électriques : le titre ne correspond donc pas au contenu.
- Les abonnés sont créables, modifiables, archivables/restaurables; ils ne sont physiquement supprimables que sans facture. Les types, devises, moyens de paiement et profils fiscaux sont modifiables/désactivables mais n’ont pas partout de suppression sûre. Les tarifs sont datés et append-only. Factures et paiements sont protégés contre la modification/suppression directe.
- La spécification de facturation existante confirme la facture combinée mais documente comme inconnus le tarif, le régime fiscal et le mode de mesure de l’eau. La demande récente confirme la facturation de l’eau au m³, sans fournir le prix ni les taxes.

## Objectifs

- Séparer les services, unités, tarifs datés, taxes et calculs; conserver USD/CDF cohérents, `Decimal` et arrondi financier explicite.
- Révision de l’exigence SEL du 23 octobre 2026 : inclure l’eau pour tous les abonnés, y compris les dossiers existants. Chaque nouveau décompte saisit les deux services dans le même formulaire et chaque facture présente les deux; `0 m³` reste une valeur explicite, visible et sans montant fictif.
- Refuser la facturation d’un volume d’eau positif lorsque le tarif ou la règle fiscale validée manque. Un relevé de 0 m³ demeure visible et facturable à 0 sans inférer de règle fiscale. Aucun tarif, taux, exemption ou assiette d’eau n’est prérempli à partir des règles électriques ou d’exemples de test.
- Enregistrer les deux services dans un snapshot de facture combinée immuable, produire prévisualisation, PDF A4, total et solde cohérents, exports et rapports corrects.
- Fournir un CRUD utilisable pour les abonnés, services, tarifs et référentiels, avec suppression physique seulement lorsque les dépendances et l’historique le permettent; utiliser archivage/désactivation ou annulation/correction traçable pour les éléments référencés.
- Préserver les factures, paiements, sauvegardes et exports historiques; toute migration SQLite est additive et idempotente.

## Hors objectifs

Pas de distribution d’eau, mesure réseau, stock, ERP, multi-poste, paiement bancaire, intégration DGI, QR fiscal, ou nouvelle architecture. Pas de re-calcul rétroactif des documents émis et pas d’effacement de données SEL sur le poste du développeur.

## Approches considérées

1. Ajouter quelques champs `water_*` et juxtaposer un second calcul spécialisé. Rapide, mais duplique les règles et rend les taxes/CRUD difficiles à faire évoluer.
2. Introduire un nouveau sous-système ou une pile distante. Plus extensible, mais incompatible avec le produit local et inutile pour deux services.
3. **Recommandée : modèle de services additif dans le socle actuel.** Étendre les plans tarifaires/profils fiscaux par service, garder les champs électriques historiques et les anciennes clés de calcul, rendre le service eau universel et stocker les nouveaux postes dans `calc_snapshot.services`. C’est le chemin le plus petit qui garde la compatibilité des anciens documents et élimine la logique électrique codée en dur.

## Décisions de conception (choix Codex sous carte blanche)

- Le mode d’entrée demandé pour l’eau est une quantité directe en m³, en parallèle de la saisie directe en kWh actuellement utilisée. Les champs de compteur/coefficients eau peuvent être conservés pour une extension de lecture, mais aucun relevé n’est requis ni affiché sans données réelles.
- L’eau est universelle au niveau du produit; toute option individuelle d’activation est supprimée. La migration met à jour les dossiers existants sans modifier les snapshots historiques.
- Les plans tarifaires sont identifiés par type d’abonné, service et date d’effet. Les règles fiscales sont propres au service; absence de configuration/confirmation bloque seulement la facturation d’une quantité d’eau positive. Les écrans exigent une confirmation explicite et une référence/source pour activer les paramètres d’eau.
- Les nouveaux snapshots ont un schéma versionné et une liste `services`; les clés de premier niveau requises par l’existant (`total`, `currency`, `energy`, `vat`, `lines`, etc.) restent disponibles pour compatibilité. Les snapshots plus anciens continuent à être rendus avec leur ancien template.
- Pour le CRUD, les référentiels inutilisés peuvent être supprimés; les éléments déjà utilisés sont désactivés/archivés. Les abonnés ayant des documents restent archivables. Les factures émises sont annulées/corrigées par remplacement; les paiements erronés sont neutralisés par une écriture de correction append-only. Aucun hard-delete en cascade de factures/paiements n’est ajouté.
- Le périmètre CRUD sera borné aux objets métier existants et nouvellement introduits; les secrets d’administration, clés de confiance, journaux techniques et sauvegardes ne deviennent pas des objets CRUD métier.

## Compatibilité et sécurité

- Déploiement de schéma par ajout de colonnes/tables/indexes, avec sauvegarde préalable, sans écraser de champs existants.
- Référentiels possédant des dépendances ne sont jamais supprimés en cascade. Les actions destructives vérifient leurs dépendances dans une transaction.
- Les anciens documents électriques restent lisibles et non modifiés. Aucun QR ni texte ne suggère une authentification DGI.
- Tout montant est calculé avec `Decimal`; aucune conversion de devise silencieuse; QR/PDF hors dépendance Internet.

## Critères d’acceptation

- Calcul et aperçu combinés; eau présente pour tous les abonnés; eau zéro visible à 0 sans prix ou taxe fictifs; quantité positive facturée seulement avec une configuration validée.
- Les paramètres d’eau sont entièrement administrables sans changement de code, mais restent inactifs jusqu’à confirmation explicite de SEL.
- Une quantité nulle peut être enregistrée sans inventer prix ni règle fiscale; une quantité positive exige toujours les paramètres eau datés et confirmés par SEL.
- CRUD des références sans suppression de données historiques référencées; correction de paiement et annulation de facture auditables.
- PDF A4 lisible avec sections indépendantes, totaux réconciliables, code QR interne seulement, anciens PDF compatibles.
- Migration ancienne base, export portable v1, installation/build Windows et tests complets réussis.

## Limitation restant métier

Le logiciel peut être prêt à recevoir la grille d’eau, mais le GO de facturation réelle de l’eau reste conditionné à la transmission par SEL du tarif par m³, de la date d’effet, de la devise/catégories applicables et des taxes/assiettes validées. Rien de cela n’est déduit d’une fixture ni de l’électricité.
