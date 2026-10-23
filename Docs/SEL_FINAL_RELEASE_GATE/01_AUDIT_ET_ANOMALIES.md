# Audit précis de départ — éléments à confirmer/reproduire

**Base technique :** PR #47, HEAD `df3be22e81068b4cbabf78afdcf92fd2cfd8bb87`. La CI `39974626585` est verte, mais n'exécute pas nécessairement des tests de conversion CDF/installation PC SEL. Ce fichier n'est pas une preuve de l'exécution des correctifs.

## P0-01 — Total CDF multiplié deux fois (défaut observé dans le code)

- Fichier `app.py`, route `invoice_pdf` (`/invoices/<id>/pdf?currency=CDF`).
- Le `for key in ('energy','local_tax','vat_basis','vat','fee','total')` multiplie déjà `calc['total']` par le taux.
- En fin de conversion, `calc['total']=str(money(Decimal(calc['total'])*rate))` le **multiplie à nouveau**.
- Test rouge minimum : 100 kWh à 0,095 USD/kWh + 16 % TVA, eau 0 m³ ; total USD = **11,02**. Taux synthétique 2 360 : total CDF attendu **26 007,20**, jamais **61 376 992,00**.
- Après correction : vérifier sous-totaux, total, montant réglé, solde, paiement net, taxe et prix unitaires de **chaque service**, exactement une conversion monétaire. Les quantités kWh/m³ et la référence QR de la facture source ne changent pas.
- Tester aussi >0 m³ (avec tarif fiscal **test**, non publié) : 100 kWh = 11,02 USD et eau 3 m³ × 0,40 USD/m³ hors taxe = 1,20 USD ; total **12,22 USD**, donc **28 839,20 CDF** à 2 360, sous réserve de fixtures à 0 % sur l'eau. Ce sont des exemples de tests, pas les tarifs officiels SEL.
- Distinguer **conversion de représentation PDF** de conversion de règlement : les données de paiement restent libellées en devise de facture; aucun taux actuel injecté dans des factures d'hier.

## P0-02 — Tarifs inactifs encore sélectionnables

- Fichier `db.py`, fonction `tariff_plan_for` : SELECT sur `customer_type`, `service_code`, `effective_from` et `confirmed`, **sans filtrage `active`**.
- Risque : une ligne `active=0` est utilisable pour facturer; l'application croit que la désactivation empêche l'utilisation.
- Norme : choisir un tarif par service/type/date, vérifier `active=1` et `confirmed=1` pour l'eau; définir explicitement le comportement si le dernier tarif applicable est désactivé (ne **pas ressusciter silencieusement** un ancien tarif).
- Couvrir `billing_period` historique/futur, plusieurs tarifs avec dates distinctes, absence de configuration, type désactivé, divergence CDF/USD et correction de tarif.

## P0-03 — Cohérence documentaire USD/CDF et services

- `app.py`, `invoice_pdf.py`, `templates/invoice_preview.html`, `templates/invoice_detail.html`.
- Les `lines` des services convertissent leurs `amount` mais ne convertissent pas systématiquement `unit_price` ni `currency` lorsque la variante CDF est produite.
- Norme : **une seule devise de présentation par document**; chaque produit quantité × prix unitaire doit être réconciliable avec ses montants suivant la politique d'arrondi; sous-totaux + taxes = total par service; somme des services = total facture; paiements nets + solde = total.
- Pour l'eau à 0 m³ sans tarif validé, afficher **« — »** au prix, montant zéro, sans afficher un taux de TVA/eau ou une exonération qui n'est pas approuvé.
- Garder le `SEL_INTERNAL_REFERENCE` du QR en **devise et montant de la facture de référence**, et le désigner comme QR documentaire non fiscal; l'option CDF ne change pas le code d'origine.

## P1-04 — Confusion « ligne eau obligatoire » et « raccordement physique »

- `db.py` force via `UPDATE clients SET water_enabled=1` et triggers après INSERT/UPDATE ; `app.py` impose `water_enabled=1` en écriture.
- La règle « deux services visibles sur toutes les nouvelles factures » **ne prouve pas** que chaque abonné possède un compteur ou branchement d'eau réel.
- Norme : séparer l'obligation d'affichage/facturation des informations de raccordement. Supprimer le mécanisme d'écrasement universel du statut métier. Conserver les données historiques et ne pas inférer rétroactivement une réalité physique non connue.
- Si la valeur `water_enabled` a déjà été écrasée par une migration intermédiaire, **ne pas deviner son ancien état**; mentionner la perte d'information potentielle et prévoir reprise via source fiable.
- La facture conserve obligatoirement une ligne 0 m³ même pour un dossier sans raccordement constaté. Ne pas facturer 0 m³ comme consommation positive.

## P1-05 — Zéro d'eau prérempli = risque d'oubli

- `templates/new_invoice.html` utilise `draft.get('water_m3','0')`, `static/desktop.js` réinitialise à '0'.
- Risque : l'agent confirme par inadvertance 0 alors qu'un relevé positif était disponible.
- Décision UX recommandée : saisie **vide par défaut**, mais 0 explicitement autorisé, accompagné d'un message clair et d'une confirmation de consommation nulle lorsque nécessaire; pas de conversion automatique blanc→zéro. Préserver brouillons erronés sans accepter une valeur absente.
- Tests du changement de client/draft : éviter de transporter le volume d'un autre client; les comportements côté UI et serveur doivent converger.
- Si SEL préfère ensuite 0 prérempli, la préférence doit être un choix **explicite**, documenté et testable, jamais la disparition de la validation serveur.

## P1-06 — Rendu trompeur des documents historiques

- `invoice_pdf.py` : pour `document_template_version==2`, le titre indique « FACTURE D'EAU ET D'ÉLECTRICITÉ » même si `calc_snapshot.services` est absent.
- Norme : ne pas étiqueter les **anciennes factures électriques** comme si l'eau avait été réellement facturée. Utiliser les données de prestations persistées du snapshot pour choisir le titre ou le modèle historique fidèle; ne **jamais** reconstituer rétrospectivement de l'eau.
- Tous les anciens PDF, depuis leurs mêmes snapshots et mêmes données, restent intelligibles, sans modification de total/numéro/échéance.

## P1-07 — Frais supplémentaires et prix de devise incohérents (à vérifier)

- `combine_invoice_calculation` construit un service électrique avec `fee` et un `total` qui peut inclure les frais, mais les tableaux de récapitulation `invoice_pdf.py` affichent surtout `subtotal`, `local_tax` et `vat` avant `total`.
- Vérifier au moins un ancien parcours `calculate_from_settings` avec `fee>0` et un parcours `FLAT_KWH`; le PDF doit permettre de comprendre toute différence, avec la ligne « frais » explicitée et sans double comptage.
- `app.py` calcule le tarif direct en `currency='USD'` malgré une configuration de tarif potentiellement en autre devise; imposer que prix, devise du tarif, devise de facture et paiements soient cohérents ou bloquer explicitement.

## P1-08 — Tests, versioning, UX et release

- Le pipeline Windows de PR #47 a réussi sur `df3be22`; ne pas extrapoler sa réussite à de nouveaux commits.
- `version.py` branche Codex représente 0.3.32, `main` représente 0.3.29; `v0.3.31` est une release brouillon. Aucune release nouvelle ne doit être annoncée.
- Le workflow build vérifie imports PyInstaller/MSI/WebView2, mais ne prouve pas un démarrage physique, un paiement réel, une impression réelle, des sauvegardes restaurables, une mise à jour de poste SEL.
- SafeUpdate `safeupdate_keys.py` a `TRUSTED_UPDATE_KEYS={}` sur main; le mécanisme production est fermé jusqu'à provisionnement réel. Ne pas lui substituer des clés de tests.
- Les PR #41 et #46 doivent être qualifiées `superseded / independent / merge needed` dans le rapport, sans fermeture/fusion automatique.

## Priorité de correction

P0-01 à P0-03 et règle « exactement 2 services » = **blocage de fusion**. P1-04 à P1-08 = blocage de livraison pilote tant que non arbitrés et testés. Si l'analyse découvre des défauts financiers, d'intégrité ou de sécurité supplémentaires, ils deviennent automatiquement P0 avec preuves de reproduction.
