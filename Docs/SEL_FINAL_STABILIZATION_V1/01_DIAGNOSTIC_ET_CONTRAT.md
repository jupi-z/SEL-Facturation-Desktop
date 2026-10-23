# SEL — Stabilisation finale du moteur commercial bi-service (contrat normatif)

**Document de travail Codex — 23 octobre 2026.** Données vérifiées dans le dépôt avant rédaction : `main@d37ec5d1`, PR [#47](https://github.com/jupi-z/SEL-Facturation-Desktop/pull/47) `monarque/water-electricity-crud@df3be22`, CI Windows réussie [39974626585](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/39974626585). **Ce diagnostic est une photographie, pas une vérité sur une branche pouvant continuer à évoluer.**

Issue de suivi : [#49](https://github.com/jupi-z/SEL-Facturation-Desktop/issues/49). Le travail antérieur [PR #41](https://github.com/jupi-z/SEL-Facturation-Desktop/pull/41) est plus ancien que #47; ne pas le fusionner par erreur ni y perdre les travaux plus récents.

## Précondition impérative

Avant de coder, **rafraîchir** `main`, la tête de #47, le statut des PRs, version, tags et rapports; examiner les différences réelles. Pour toute exigence déjà satisfaite et prouvée, noter PASS et ne pas modifier par habitude. Corriger exclusivement les défauts encore présents. La branche d'implémentation doit partir de l'état le plus récent **de #47** en préservant tous ses changements ; poursuivre #47 si possible, sinon ouvrir une PR dépendante sur #47 et décrire clairement le plan de fusion. **Ne pas redémarrer de `main` en perdant les travaux de #47.**

## Objectif produit NON NÉGOCIABLE

SEL vend **l'électricité au kWh** et **l'eau au m³** dans **une seule nouvelle facture avec un seul numéro et un seul total**. Toutes les nouvelles factures contiennent les **deux lignes**, pour **tous les abonnés** : 0 m³ doit apparaître **0 m³ / 0,00** et ne jamais être omis. Le volume d'eau positif exige **tarif, catégorie, date d'effet, devise et fiscalité conformes à une décision SEL explicite**; aucun tarif, taux, exonération, dispositif DGI ou référence fiscale ne peut être inventé. Le QR reste **interne SEL, non fiscal**, sans données personnelles inutiles.

**Distinguer deux concepts :**
- `invoice_service_required` = oui, les deux prestations s'affichent sur toute nouvelle facture.
- `water_connection_status` = état réel du raccordement (connecté, non raccordé, inconnu selon modèle compatible). Ce n'est **pas** un critère de suppression de la ligne d'eau.

La branche #47 actuelle force `water_enabled=1` pour tous les clients via migration et triggers; cette opération efface une éventuelle distinction de raccordement pour les comptes existants. Évaluer les données déjà en circulation et introduire une séparation rétrocompatible; **ne pas détruire de données** pour satisfaire une règle d'affichage. Un statut de raccordement `unknown` est préférable à une affirmation factuelle non prouvée. Pour les anciennes bases où les valeurs ont **déjà** été forcées et l'état perdu, **ne pas inventer le statut antérieur**; documenter les limites de récupération depuis sauvegardes.

## Défauts ciblés — ordre de priorité

### P0-A — Justesse USD/CDF : voie du PDF et montants dérivés
- `app.py`, route `invoice_pdf` : `calc['total']` est multiplié dans la première boucle, puis **encore** multiplié après le traitement de `services`. Pourtant le `inv['calc_snapshot']` qui est transmis au renderer est sérialisé **avant** le changement des services et avant la deuxième multiplication. Il faut donc distinguer **calcul intermédiaire erroné** et **sortie observable**; **ne pas affirmer que le PDF affiche le total doublement converti sans preuve**.
- Risque observable fort : snapshot PDF sérialisé avec `currency='CDF'` et valeurs de haut niveau converties, mais **détails `services` initialement USD**, prix unitaires non convertis, sous-totaux affichés avec une devise incohérente, paiements convertis indépendamment. Inspecter la chaîne de production jusqu'au texte extrait du PDF réel.
- Contracter une fonction pure/testable pour conversion de **copie d'affichage** (pas l'original SQLite) : chaque **montant monétaire** est converti une fois, **aucune quantité** n'est convertie, la monnaie affichée et les prix unitaires concordent, les arrondis sont déterministes, le solde = total - encaissé net.
- Clarifier le **statut commercial du document CDF** (variante indicative ou document payable) avant de modifier son sens. La source du taux, sa date d'application et la possibilité d'un taux en query-string doivent être explicitement contrôlées; si l'agent peut choisir un taux, tracer cette décision sur le PDF et interdire des variantes contradictoires non identifiables. Préserver le QR de **référence du document original USD**, ou définir une représentation stable non trompeuse et testée.
- N'introduire aucune conversion USD→CDF implicite dans les snapshots des factures émises. Éviter de stocker la même facture deux fois.
- Confirmer le traitement des **arrondis par ligne**, le rapprochement entre total et somme des lignes, et la gestion de paiements partiels/corrections pour plusieurs devises.

### P0-B — Sélection des barèmes réellement applicables
- `db.tariff_plan_for()` cherche `customer_type`, `service_code` et `effective_from<=?` mais **n'exclut pas** `active=0`. Corriger la sélection en fonction de la politique historique : pas de tarif désactivé utilisable dans une **nouvelle** émission; service/catégorie/date/confirmation cohérents.
- Tester : plusieurs versions datées, futur non encore effectif, barème le plus récent désactivé, barème valide plus ancien, absence totale de barème actif, absence de confirmation pour l'eau, service incorrect, catégorie inactive, devise différente. Expliquer si une désactivation du tarif le plus récent signifie retour au précédent ou interdiction explicite; ce **choix doit être cohérent, traçable et testé**. Ne jamais ressusciter silencieusement un tarif censé être révoqué.
- Vérifier le comportement de `current_tariffs()`, paramètres, sélection électrique directe et lignes d'eau : la correction doit être uniforme. Les factures **historiques** gardent leurs snapshots tarifaires.

### P0-C — Intégrité des factures et historique
- Nouvelles factures : exactement deux services `ELECTRICITY` et `WATER`, quantité 0 m³ ≠ champ absent; 0 m³ accepté même sans prix d'eau validé **sans inventer** prix/taxe/assiette/exonération. Montant 0,00 si aucun montant fixe réel confirmé; eau positive refusée si tarif ou règle fiscale manque, est inactif ou incompatible.
- Vérifier le **même** calcul entre prévisualisation et émission; les modifications concurrentes de catalogue/client et l'émission de deux documents pour une période sont contrôlées; les anciennes sessions ne créent jamais de facture électrique seule.
- Ne pas altérer `calc_snapshot`, `client_snapshot`, numéro, paiements, contre-écritures des documents existants. PDF historique = libellé fidèle aux prestations **effectivement archivées**, même s'il a `document_template_version=2` avec un ancien titre combiné trompeur; traiter la présentation sans réécrire le snapshot ni afficher une fausse consommation d'eau.
- Garder l'export `sel-facturation-portable` `schema_version=1` **compatible avec son contrat** : champs existants préservés, ajouts seulement compatibles/explicitement documentés; conserver exports CSV et éventuelles contre-écritures.
- La sauvegarde SQLite cohérente, les triggers d'immuabilité, le contrôle d'une seule facture active par période, les règlements net et l'impression existante restent intacts.

### P1-A — Zéro eau saisi intentionnellement
- L'actuel `templates/new_invoice.html` fournit `value="0"` par défaut; un agent distrait pourrait publier 0 alors que le relevé est inconnu. Préférer **champ vide** ou **confirmation explicite de 0 m³ dans la prévisualisation** avec contrôle serveur, selon l'UX la plus simple et testable. Toujours permettre une vraie valeur zéro sans complexité disproportionnée.
- Le passage d'un client à un autre ne doit pas conserver l'eau positive/ancienne sans nouvel assentiment. Des champs absents ou invalides doivent refuser l'émission; le formulaire accessible/clavier préserve le brouillon en cas d'erreur mais ne réutilise pas un ancien accord.
- Ne pas demander au client final de certifier l'absence de consommation : c'est une validation **opérateur SEL**.

### P1-B — Information raccordement et contrat
- Ne pas lier la visibilité de `WATER` au raccordement réel. Ne pas forcer `water_enabled=1` avec triggers comme **preuve factuelle**. Proposer un attribut `water_connection_status` indépendant ou, si le champ est purement technique, le renommer/déprécier avec migration réversible. Éviter de réintroduire une option pouvant cacher l'eau sur une facture.
- Les dossiers existants dont l'état était 0 ou inconnu ne doivent pas être qualifiés de « raccordés ». Garder une voie de saisie manuelle du statut de raccordement seulement si justifiée métier, et **jamais bloquante pour l'affichage**.
- Vérifier la compatibilité SQLite additive, contrôles d'insertion/migration, trigger et réinstallation. **Si la perte d'information de la branche actuelle est déjà sur un PC**, avertir et prévoir sauvegarde/restauration spécifique avant toute nouvelle migration.

### P1-C — Rapports, PDF, cohérence et documentation
- Rapport de PR #47 et `Docs/SEL_EAU_ELECTRICITE_CRUD_RAPPORT.md` contiennent encore des éléments contradictoires 0.3.30, 0.3.31, « 0.3.32 en cours » et « facture électrique seule » ; réécrire une synthèse datée basée sur commit / tests réels. Ne pas effacer l'historique des diagnostics sans l'archiver.
- Tester valeurs d'eau positives et nulles, taux nul **autorisé uniquement si confirmé**, TVA/assiettes distinctes, total brut/net, export, dashboard, facture annulée, ancien document et QR interne.
- Le rendu PDF A4 doit être inspecté à 100 % en cas d'eau positive, eau nulle, ancien template, CDF et données longues/multi-pages; pas de collisions ni zones non imprimables; test **papier** à faire sur PC cible.

## Hors périmètre
Pas de refonte UI générale, application Web publique, monolithe NestJS, intégration DGI, QR fiscal, ERP, calcul artificiel de taxes, nouveau service distant, service Windows privilégié ou contournement UAC/SmartScreen. Ne pas toucher à SafeUpdate hors régression provoquée par la fusion; ce canal est **fail-closed** sans clé publique Ed25519 de production.

## Autorisations et limites
Codex : autorisé à modifier le code sur branche de travail, ajouter tests, exécuter CI, créer un rapport et une PR non fusionnée. **Non autorisé** à fusionner main, publier une version, modifier une clé privée de production, installer sur un poste SEL, supprimer des releases ou restaurer destructivement des données réelles sans une nouvelle instruction explicite du propriétaire.

Un test automatisé passant n'est pas une homologation fiscale, une validation tarifaire SEL ou une recette imprimante.
