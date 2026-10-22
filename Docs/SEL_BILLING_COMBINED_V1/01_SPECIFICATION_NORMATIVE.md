# SEL Facturation Desktop — Spécification normative : facture unique eau + électricité

**Version du dossier :** 1.0 · **Date :** 22 octobre 2026  
**Dépôt cible :** `https://github.com/jupi-z/SEL-Facturation-Desktop`  
**Base vérifiée en lecture seule :** `main@baf7a88aae9696b29dff4fd45919daad861c9d7a`  
**Statut :** cahier des charges pour Codex ; **aucune modification du dépôt n'a été effectuée lors de sa rédaction**.

## 0. Hiérarchie des règles

- **VALIDÉ PAR LE DONNEUR D'ORDRE** : une facture/document unique regroupant **eau et électricité**, mise en page professionnelle SEL en A4, QR de **référence interne** distinct du futur QR fiscal, préservation de l'existant, modifications strictement centrées sur la facture.
- **PROPOSÉ (à réaliser au plus simple)** : rendu PDF vectoriel, sections eau et électricité, configuration minimale des données réellement facturées, snapshot immuable, tests spécifiques.
- **À CONFIRMER PAR SEL** : régime des prestations d'eau, compteur ou forfait, tarif eau, unités, TVA et autres taxes de l'eau, détail de la mention manuscrite « 05 », catégories d'abonnés, modalités de facture commerciale et de remise DGI.
- **CONTRAINTE NON NÉGOCIABLE** : aucune donnée, tarif, taxe, signature, certification fiscale, référence DGI ou code fiscal inventés.

`DOIT`/`INTERDIT` désigne une exigence impérative ; `DEVRAIT` une recommandation ; `PEUT` une amélioration facultative. Aucun ajustement de code au-delà du périmètre facturation et de ses dépendances immédiates sans justification dans la PR.

## 1. Mission exacte et limites

**Objectif fonctionnel :** pour un abonné et une période, produire **un seul document SEL**, avec les postes électricité et eau individualisés, leurs unités, tarifs, bases fiscales validées, sous-totaux, un total commun, l'historique des règlements et une impression A4 lisible.

**Ce projet n'est pas** une plateforme de distribution multi-compteurs, un ERP, une application web publique, une certification DGI, ni Monarque Connect. Ne pas implémenter de module métier général eau, de nouveau système de comptes, de CRM, de paiement en ligne, de mise à jour automatique ou de refonte de navigation à l'occasion de ce chantier.

**Stratégie de livraison :**
1. **Étape A — adaptation du document PDF et de sa structure de données** : supporter une représentation combinée, maintenir les anciens décomptes électriques, ajouter le QR interne ; impossible de renseigner un poste eau réel sans les données métier.
2. **Étape B — données d'eau minimales, uniquement lorsque SEL les valide** : saisie du volume ou du forfait, configuration du prix et des taxes qui lui sont spécifiques, prévisualisation et émission combinée. Étendre uniquement la surface « Nouvelle facture »/« Prévisualisation »/« PDF » et la configuration strictement nécessaire.
3. Pas de publication automatique en production. Une PR et une recette factuelle précèdent tout installateur destiné à SEL.

**Cas bloquant :** en mode combiné, si l'eau manque d'une règle de calcul ou de données fiables, empêcher l'émission définitive ; proposer une simulation non enregistrée explicitement fictive uniquement pour les tests. Ne jamais ajouter arbitrairement « Eau : 0,00 » à une vraie facture pour prétendre qu'elle est combinée. La facturation électrique historique demeure disponible selon son comportement validé, en mode de compatibilité.

## 2. État du code constaté et zones concernées

Sur le commit de référence, le socle :
- `invoice_pdf.py` : rendu ReportLab A4, titre actuel `DÉCOMPTE D’ÉLECTRICITÉ`, tableaux de consommation kWh et récapitulatif ; **pas de section eau ni de QR**.
- `domain.py` : `calculate_flat_rate`, `calculate_from_readings`, `calculate`, gestion monétaire en `Decimal`, tarifs kWh, TVA liée à l'énergie ; **pas de calcul du service eau**.
- `app.py` : `new_invoice`, `confirm_invoice`, `invoice_pdf`, saisie de `consumption_kwh`, prévisualisation ; conversion de présentation CDF ; **aucune saisie de l'eau**.
- `db.py` : `clients`, `invoices`, `payments`, `tariff_plans`, paramètres et instantanés `client_snapshot`, `company_snapshot`, `calc_snapshot`; index unique pour une facture active par client/période.
- `templates/new_invoice.html`, `templates/invoice_preview.html`, `templates/invoice_detail.html` : parcours facturation existant à préserver.
- `tests/test_invoice_pdf_tariffs.py`, `tests/test_storage_pdf.py`, `tests/test_consumption_billing.py`, `tests/test_consumption_http.py`, `tests/test_invoice_preview.py` : tests de non-régression à renforcer.
- `static/sel-icon.png` : identité visuelle existante, à conserver ; ne pas rechercher/remplacer le logo sans instruction explicite.

Codex DOIT relire `AGENTS.md`, `Docs/CODEX_HANDOFF.md` et l'état réel du dépôt **au moment où il commence**, car des commits peuvent avoir été ajoutés après cette vérification. La liste ci-dessus est une base, pas un mandat de modifier tous ces fichiers.

## 3. Lecture des annotations SEL

| Élément photo | Exigence d'interface | Précaution |
|---|---|---|
| Inscription manuscrite évoquant « facture d'eau et d'électricité » | Titre et contenu couvrent les deux services | **Document commercial** : libellé définitif « Facture » ou « Décompte » à valider selon le circuit fiscal ; pas « Facture normalisée » sans dispositif officiel |
| « Résidentiel » rayé | Supprimer la répétition de `Type d'abonné` en colonne de droite et conserver le type issu de la fiche client une seule fois | Ne pas supprimer globalement la catégorie ni la remplacer par une valeur devinée |
| « 05 » près du point de service | Champ `point de service` tel qu'enregistré ; ne pas coder `05` en dur | Sens exact du nombre à confirmer |
| Notes eau / électricité | Deux sections explicites avec unités distinctes (`kWh` et `m³` si eau mesurée) | Aucune conversion abusive kWh ↔ m³ |
| « Tarif unitaire HT » barré | Supprimer la **ligne isolée redondante** du récapitulatif | Garder le prix unitaire dans **chaque ligne de détail** et les montants HT |
| Rectangle « QR Code » | QR de référence SEL, visuellement identifié | Jamais de QR prétendu DGI ; zone fiscale future séparée |

Les annotations manuscrites peu lisibles ne constituent pas des exigences définitives ; ne pas automatiser des suppositions.

## 4. Modèle du document A4

### 4.1 En-tête (répété de façon cohérente en multipage)

- Logo actuel SEL, nom légal **tel que configuré** et contacts connus seulement.
- RCCM, NIF, identification nationale si existants ; aucune valeur fictive ni champ prérempli avec les données d'une capture d'essai.
- Titre orienté service : **« DÉCOMPTE DE CONSOMMATION — EAU & ÉLECTRICITÉ »** comme valeur transitoire sûre ; « FACTURE D'EAU ET D'ÉLECTRICITÉ » est une variante candidate à faire valider par SEL et à utiliser seulement avec les vérifications fiscales appropriées. Ne jamais imprimer « DGI certifié », « normalisé » ou similaire sans preuve.
- Référence SEL, date d'émission, période, échéance. Conserver une référence **unique** pour le document combiné.

### 4.2 Client / contrat

- Nom/raison sociale, code abonné, référence du contrat, adresse, catégorie/type si connu et utile, NIF client **s'il existe** ; ne pas inventer un NIF.
- Centre, point de service (reprendre le champ réel), coordonnées nécessaires ; pas de répétition de « résidentiel » dans plusieurs cellules.
- Type de prestation/contrat si SEL valide une nomenclature.

### 4.3 Section « ÉLECTRICITÉ »

- Mode de calcul réel : consommation directe (`kWh`) OU compteur et index (index précédent/actuel + coefficient) ; n'afficher aucun faux index en mode direct.
- Consommation, unité kWh, période, libellé du tarif, quantité, prix unitaire HT, devise, montant HT, taxe(s) propres et total service.
- Utiliser les tarifs, catégories et taux réellement enregistrés en snapshot ; conserver la logique déjà utilisée en production (actuel : USD/kWh dans les plans tarifaires). Ne pas remplacer de facto les barèmes validés.
- Les frais particuliers ne sont affichés que s'ils sont configurés et applicables.

### 4.4 Section « EAU »

- Mode choisi selon SEL : `METER_M3` (index précédent/actuel × coefficient), `DIRECT_M3` (volume direct), `FLAT_FEE` (forfait) ; **aucun mode n'est réputé actuellement validé**.
- Si compteur/volume : unité `m³`, volume, éventuels index/numéro de compteur eau si SEL les fournit ; tarif par m³ et montant HT.
- Si forfait : libellé du forfait, quantité de périodes/forfaits, prix, montant HT ; **ne pas afficher de faux m³**.
- Taxes et exonérations propres au service d'eau, configurées indépendamment de l'électricité et issues de règles confirmées. **Ne pas appliquer automatiquement TVA 16 % à l'eau**.
- Séparer données eau et électricité ; la référence contrat globale peut être partagée si SEL le confirme.

### 4.5 Récapitulatif unifié

1. Sous-total électricité HT.
2. Sous-total eau HT.
3. Frais additionnels validés, ventilés par service si nécessaires.
4. Bases et montants de taxe par service (par exemple TVA électricité / TVA eau si réellement applicable), autres taxes si configurées ; montant non taxable s'il existe.
5. Total de toutes les composantes en **une devise de facturation cohérente**.
6. Paiements cumulés et solde à régler, dans la **même devise** ; date d'échéance et modes de règlement connus.

Si une variante CDF est affichée à partir d'une facture USD : montrer qu'il s'agit d'une **conversion de présentation**, avec taux, date et devise source ; le document original conserve sa devise et ses montants. La somme des valeurs converties et les arrondis doivent se réconcilier. Ne pas additionner USD et CDF. Un taux non vérifié bloque la variante CDF sans bloquer le PDF original USD.

### 4.6 Pied de page et QR

- QR interne en bas, dans une zone lisible et avec légende « **Référence interne SEL — non fiscal** » ou « **Référence du décompte SEL** » (si distinction claire dans documentation). La formule courte « Référence interne SEL » est suffisante sur le PDF, sans avertissement anxiogène.
- Le futur QR DGI doit avoir un emplacement **conceptuellement indépendant**. **Ne pas afficher un QR DGI factice ni une zone vide intitulée DGI** avant réception d'éléments fiscaux officiels.
- Contacts et informations complémentaires, pagination, référence sur chaque page, état « ANNULÉ » clairement visible si document annulé.
- Zéro chevauchement ; pas de lignes orphelines, tableaux tronqués ou QR écrasé. La mise en page vectorielle ReportLab garde du texte sélectionnable.

## 5. Contrat de données combinées : ajout minimal non destructif

### 5.1 Préservation obligatoire

- `invoices.number`, statut, client/période, ancienne structure des snapshots et paiements restent inchangés.
- `calc_snapshot` des anciens documents : **ne jamais réécrire** ni recalculer ; le moteur PDF sait les afficher selon leur format historique.
- Nouveaux décomptes combinés : ajouter une clé optionnelle `services` ou `components` dans le **snapshot de calcul** (le choix exact doit être documenté, stable et testé), sans supprimer les clés historiques nécessaires au reste de l'application, notamment `total`, `currency`, `vat`, `energy`, `lines` le cas échéant.
- Export JSON existant `sel-facturation-portable`, `schema_version: 1` : conserver l'enveloppe, les références et anciens champs ; toute extension additionnelle doit être décrite. En cas de changement non rétrocompatible, publier **une version 2 séparée** et maintenir l'export v1.
- Ne pas modifier le schéma SQLite sans nécessité. Si configuration eau ajoutée, préférer configuration additive et snapshots ; migrations idempotentes sur base ancienne + sauvegarde préalable.

### 5.2 Modèle logique recommandé pour une nouvelle facture

Exemple purement **structurel**, valeurs non officielles et non liées à un vrai client :

```json
{
  "document_kind": "COMBINED_WATER_ELECTRICITY",
  "schema_version": 2,
  "currency": "USD",
  "services": [
    {
      "code": "ELECTRICITY",
      "mode": "DIRECT_KWH",
      "quantity": "100",
      "unit": "kWh",
      "unit_price_excl_tax": "0.095",
      "subtotal_excl_tax": "9.50",
      "taxes": [{"code": "VAT", "rate": "16", "basis": "9.50", "amount": "1.52"}],
      "total_incl_tax": "11.02"
    },
    {
      "code": "WATER",
      "mode": "DIRECT_M3",
      "quantity": "10",
      "unit": "m3",
      "unit_price_excl_tax": "0.40",
      "subtotal_excl_tax": "4.00",
      "taxes": [],
      "total_incl_tax": "4.00"
    }
  ],
  "subtotal_excl_tax": "13.50",
  "tax_total": "1.52",
  "total": "15.02"
}
```

**Attention :** `0.40 USD/m³` et `taxes: []` pour l'eau ne sont QUE DES FIXTURES DE TEST. Le moteur ne DOIT PAS les utiliser par défaut en production. La TVA et les autres taxes eau demeurent inconnues jusqu'à confirmation SEL. La structure peut être ajustée si Codex constate qu'un autre format additif est plus compatible avec `domain.py` ; les invariants priment sur ce JSON d'illustration.

### 5.3 Contrôles métier

- Deux lignes service distinctes `ELECTRICITY` et `WATER` obligatoires pour une **nouvelle facture combinée** après activation de ce mode.
- Aucun volume négatif, taxe non finie, tarif zéro non expressément autorisé, devise incohérente, période invalide ni total négatif.
- En l'absence de règle SEL validée pour l'eau, **aucune émission combinée réelle** ; les anciennes factures d'électricité restent opérationnelles.
- Avant enregistrement : prévisualisation fidèle des mêmes postes et sous-totaux ; à confirmation : recalcul serveur ou snapshot approuvé, transaction atomique, contrôle d'unicité (client, période) déjà en place.
- Le QR est dérivé exclusivement d'un document effectivement enregistré. Un brouillon n'a pas de QR définitif ni de numéro réservé.
- Le paiement est saisi sur la facture **unique**, non séparément par section dans cette V1.

## 6. QR code interne : contrat **SEL_INTERNAL_REFERENCE v1**

**But :** identification et rapprochement manuel avec le document, **pas** preuve cryptographique ni authentification par la DGI.

Recommandation : JSON UTF-8 encodé tel quel dans le QR, champs fixés :

```json
{"kind":"SEL_INTERNAL_REFERENCE","v":1,"issuer":"SEL","number":"SEL-2026-000003","issued":"2026-10-20","currency":"USD","total":"43.16"}
```

Ces valeurs illustrent les **données visibles sur la photo** et ne constituent pas une copie à injecter dans le logiciel. À l'exécution, prendre exclusivement les valeurs du **snapshot de la facture courante**. Respecter le vrai préfixe de numérotation configuré (ne pas figer `SEL-2026-...`).

- Données incluses : identifiant du schéma, version, marque SEL, numéro documentaire, date, devise, montant TTC initial.
- Données exclues : nom, adresse, téléphone, NIF ou identifiant personnel du client, coordonnées du compteur, identifiants de session, secrets, jetons, RIB, lien vers un serveur inexistant.
- Ne pas interpréter ce QR comme reçu de paiement : le solde change, **le montant initial et le QR doivent rester constants** pendant les paiements et réimpressions.
- Générer localement, sans service web tiers ; utiliser de préférence `reportlab.graphics.barcode.qr.QrCodeWidget` pour un QR vectoriel, sans dépendance superflue. Dimensions lisibles et marge blanche conformes au rendu du composant ; tests scanner requis.
- Libellé imprimé : « Référence interne SEL » et **jamais « QR DGI »**, « Authentifié » ou « Vérifiez auprès de la DGI ».
- En cas de problème technique d'encodage, ne pas rendre illisible le reste de la facture. Signaler un défaut explicite, et empêcher la remise tant qu'un QR obligatoire ne peut être produit/lu.
- Le futur QR fiscal sera alimenté **uniquement par un système fiscal officiel/homologué** et stocké séparément, selon son schéma officiel. Pas de substitution, copie du QR interne ni transformation en faux QR fiscal.

## 7. Règles de calcul et d'arrondi

- Tous les calculs monétaires : `decimal.Decimal` ; **jamais** `float` pour prix, taxes, bases et montants.
- Conserver les précisions des prix unitaires (ex. 0,095 USD/kWh) et arrondir les montants facturés selon la règle documentée, cohérente avec le comportement existant (`ROUND_HALF_UP` à 0,01 pour les montants).
- Séparer base taxable/non taxable et taxes **par service**, puis totaliser les montants arrondis selon un ordre explicite et constant.
- Les régimes d'eau et d'électricité ne sont pas interchangeables. Toute règle d'eau non confirmée est bloquante pour le mode combiné réel.
- L'exemple électricité vérifié : `391.64 kWh × 0.095 USD = 37.2058 USD`, arrondi `37.21 USD`, puis 16 % sur 37.21 donne `5.95 USD`, total `43.16 USD`. La facture photo affiche ces chiffres ; ils servent de test du **moteur actuel**, pas de barème eau.

## 8. Compatibilité, sécurité et fiabilité

- Aucune modification des totaux, taxes et lignes d'une ancienne facture.
- Les factures annulées restent annulées et marquées en PDF ; ne pas réattribuer le numéro.
- Une seule facture active par abonné/période reste la règle, sauf demande métier explicite contraire de SEL.
- Les exports clients/factures/paiements CSV et JSON doivent rester exploitables ; déclarer les nouvelles clés optionnelles.
- Pas de nouvelle dépendance réseau, SaaS, CDN ou serveur distant pour le QR et le PDF.
- Préserver l'application native pywebview et les comportements d'impression Windows, de sauvegarde, d'installation et de mise à jour.
- Pas de données personnelles inutiles dans le QR ; aucun enregistrement de mot de passe dans les exports.
- Protection contre le texte malveillant (ReportLab `Paragraph` : échapper toutes les données utilisateur), PDF multipage et textes très longs.
- Les coordonnées, NIF et autres informations réglementaires ne doivent pas être inventés. Si nécessaires et manquants, blocage contrôlé ou validation de l'entreprise, pas de remplacement arbitraire.

## 9. Frontière fiscale et sources officielles

La DGI indique que la facture normalisée comporte l'identité des parties, RCCM/NIF/adresse, date et numéro, désignation/quantité/prix, montants HT/TVA/HTVA, taxes éventuelles et éléments de sécurité fiscaux (identifiant DEF, code d'authentification et QR). Le logiciel local **ne possède pas par ce seul changement ces éléments fiscaux officiels**. Le processus d'homologation/fiscalisation est séparé.

Sources vérifiées (22/10/2026) :
- DGI — Réforme de la facture normalisée : https://dgi.gouv.cd/reforme-de-la-facture-normalisee/
- DGI — Plateforme e-DEF : https://edef.dgirdc.cd/
- DGI — Communiqués officiels : https://dgi.gouv.cd/communiques-officiels/
- ARE — arrêté nº084 du 27 décembre 2018, article 19 (facturation électrique basse tension), pour vérification sectorielle avec SEL : https://are.gouv.cd/wp-content/uploads/2021/07/Arrete-n%C2%B0-084-Termes-Contrat-dabonnement-27.12.2018.pdf

**Ne pas confondre** « mentions commerciales et fiscales renseignées » avec « facture normalisée authentifiée ». Ne pas ajouter de bannière anxiogène sur le document client. Les avertissements sur la conformité peuvent rester dans les documents techniques ou le contrôle administratif interne.

## 10. Livrables techniques attendus de Codex

1. Audit d'écart entre la présente spécification et `main` au moment de l'exécution : changements strictement nécessaires et points non confirmés.
2. Modèle logique minimal et implémentation additive du snapshot combiné ; adaptateur de lecture pour les anciens snapshots.
3. Rendu PDF unique, vectoriel, stable à l'impression, avec service électricité + eau + total + QR interne.
4. Configuration et saisie eau seulement selon données et règles **confirmées**, sans fausses valeurs ni refonte globale.
5. Tests automatisés (matrice du document `02_TESTS_ET_RECETTE.md`) ; tests de QR décodable et rendu visuel A4.
6. Exemple(s) de PDFs **synthétiques** à valider visuellement, étiquetés dans les artefacts de test, jamais dans la base réelle SEL.
7. PR avec changelog, preuve des tests, compatibilité migration/sauvegardes, incertitudes, recommandations GO/NO-GO.
8. **Ne pas créer/publier de release** sans instruction explicite du donneur d'ordre et validation Windows cible.

## 11. Interdictions d'extension opportuniste

Aucune refonte de l'authentification, dashboard, clients/CRM, gestion de stock, intégration DGI, logiciel ERP, facturation massive, paiement mobile, architecture multi-tenant, intégration Monarque Connect, nouvelle marque, nouveau logo ou site web dans cette mission. Corriger uniquement les défauts bloquants directement découverts sur le parcours de facture, en les justifiant.