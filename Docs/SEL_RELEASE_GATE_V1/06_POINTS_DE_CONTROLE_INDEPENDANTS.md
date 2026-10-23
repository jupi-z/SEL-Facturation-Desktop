# SEL Release Gate V1 — points de contrôle indépendants et ambiguïtés éliminées

**Ajout d'audit du 23 octobre 2026**, complément au [contrat canonique](02_EXIGENCES_CORRECTIVES.md) et à sa [matrice](03_MATRICE_DE_TESTS.md). Ne remplace pas ces documents. Issue de suivi : [#49](https://github.com/jupi-z/SEL-Facturation-Desktop/issues/49).

## 1. Bug de conversion CDF — preuve avant annonce
La route `app.py:invoice_pdf` de la PR #47 à `df3be22` :
1. Convertit une première fois les clés `energy,local_tax,vat_basis,vat,fee,total` dans une **copie** `calc`.
2. Sérialise `inv['calc_snapshot']=json.dumps(calc)`.
3. **Après** la sérialisation, modifie les montants de `calc['services']` sans resérialiser la copie transmise au renderer.
4. Multiplie `calc['total']` **une deuxième fois** dans la variable Python; ce deuxième résultat ne se retrouve pas forcément dans le PDF, car la sérialisation date de l'étape 2.

**Ne pas accuser le PDF de présenter 61 376 992 CDF sans extraire les chiffres du document réel.** Le risque **directement contrôlable** est que les postes/services USD restent chiffrés en USD avec une devise PDF indiquant CDF, tandis que le total source de 11,02 USD est correctement converti une première fois en 26 007,20 CDF. Le résultat exact doit être attesté en exécutant `GET /invoices/<id>/pdf?currency=CDF` sur une base temporaire puis extraction `pypdf.PdfReader` + rapprochement arithmétique.

Exemple **uniquement synthétique** : 100 kWh × 0,095 USD = 9,50 USD HT; TVA électrique 16 % = 1,52 USD; eau = 0 m³; total 11,02 USD; à 2 360 CDF/USD, total **26 007,20 CDF**, service électrique **26 007,20 CDF**, service eau **0,00 CDF**, encaissement 3 USD → reste 8,02 USD soit **18 927,20 CDF**. Conversion des quantités **interdite**. Prix unitaires et sous-totaux affichés avec leur vraie monnaie, aucune ligne « 9,50 CDF » issue d'un subtotal USD.

## 2. Tarifs désactivés — refus explicite
Dans `db.tariff_plan_for()`, une recherche `service_code/customer_type/effective_from` ne filtre pas `active=1`. **Tester en rouge** un tarif inactif toujours sélectionné; puis faire appliquer une politique stable. Politique conservatrice recommandée dans [02_EXIGENCES_CORRECTIVES](02_EXIGENCES_CORRECTIVES.md) : le dernier tarif applicable étant désactivé, refuser la facturation au lieu d'employer silencieusement un ancien tarif. Tester aussi la vue `current_tariffs()` et le parcours qui appelle un tarif électrique direct.

## 3. Raccordement physique != ligne d'eau
La branche #47 introduit `UPDATE clients SET water_enabled=1` et triggers qui imposent 1 aux comptes existants. **L'obligation de voir « 0 m³ » ne prouve pas que l'abonné soit raccordé physiquement.** Préserver les anciens indicateurs, ne pas modifier rétroactivement les observations métier. La règle de rendu de `services` est séparée de cette information. Les valeurs historiques déjà écrasées ne peuvent pas être reconstituées par une hypothèse; si nécessaire restauration depuis copie ancienne contrôlée ou marquage inconnu.

## 4. Zéro eau : détecter l'oubli de saisie
Le formulaire `templates/new_invoice.html` affiche initialement `water_m3=0`. Une confirmation d'absence de consommation doit être **intentionnelle**, idéalement lors de la prévisualisation. Un choix de client ne doit pas hériter d'une quantité positive ou d'un accord zéro de l'ancien client. La page serveur doit contrôler cet accord; éviter qu'un champ invisible ou un état JS suffise.

## 5. Documents anciens et PDF
- `invoice_pdf.py` affiche parfois un titre combiné selon la version du template plutôt que le contenu du snapshot. Tester anciens documents mono-service, **sans inventer une eau historique** ni réécrire SQLite.
- Dans le flux CDF, QR interne doit rester rattaché à la facture source, avec libellé « référence interne » et sans suggérer une certification DGI.
- Tests `test_invoice_pdf_tariffs.py` qui fournissent un **snapshot CDF déjà fabriqué** ne valident pas le bug de la route CDF : il faut un **test HTTP traversant toute la conversion**.

## 6. Conditions de réussite
- Rejouer **suite entière** + CI Windows du **SHA final**, et preuves exactes PDF/USD/CDF, données/sauvegardes, packages.
- P0 encore rouge ou comptabilité non réconciliée => **NO-GO technique**.
- Absence de validation tarif eau réelle et de test matériel/imprimante SEL => **NO-GO commercial**, même si tests automatiques PASS.
- SafeUpdate : exécutable Windows non signé **autorisé dans les tests**, mais manifeste Ed25519 de production absent => **NO-GO auto-update production**, jamais bypass.
- Mettre à jour le rapport de PR et [issue #49](https://github.com/jupi-z/SEL-Facturation-Desktop/issues/49) avec preuve; **ne pas fusionner, publier, ou déployer sans l'autorisation explicite du propriétaire**.
