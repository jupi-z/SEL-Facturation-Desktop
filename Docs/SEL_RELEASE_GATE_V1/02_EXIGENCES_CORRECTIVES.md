# SEL Release Gate V1 — spécification normative de correction

**Portée :** facturation eau + électricité obligatoire, tarifs confirmés, rendu et transactions exacts, données préservées. Ne pas introduire de nouveau module complet ou de refonte d'architecture.

## P0 — Montants et devise, exactitude totale

**FX-01 :** Chaque montant monétaire imprimé en CDF sur la variante d'une facture USD doit être cohérent avec le taux applicable et avec son libellé de devise. Séparer clairement les données source immuables du modèle de présentation converti. Une facture et ses services doivent satisfaire `somme(total_services) == total_document`, `somme(lignes) == sous_total_service` et `total - paiements_nets == solde` (avec arrondis documentés).
**FX-02 :** Corriger dans `app.py` l'ordre transformation / sérialisation du snapshot d'affichage; **une seule conversion** pour chaque valeur, y compris total, énergie, taxe locale, TVA, frais, lignes, paiements/corrections et sous-totaux eau/électricité. Proscrire deuxième multiplication et copie stale. Vérifier pourquoi `inv['calc_snapshot']` est défini avant les modifications `services`. Mutation exclusivement dans une **copie non persistée** pour le PDF CDF.
**FX-03 :** Préserver la quantité dans ses unités physiques : kWh et m³ **ne se convertissent pas**. Pour le prix unitaire, choisir une politique cohérente et documentée : (A) prix unitaire converti CDF/unité et montants CDF, OU (B) prix source USD/unité étiqueté USD et montants CDF clairement qualifiés après conversion. L'option A est préférable pour une facture présentée intégralement en CDF. Ne jamais étiqueter USD un montant CDF ni l'inverse.
**FX-04 :** Les frais fixes et taxes éventuels restent distincts; pas de duplication ni disparition dans les `services`. Le taux appliqué à l'aperçu doit être traçable (valeur, date, source) et le QR interne reste une **référence du document source non fiscal**. Aucune altération SQLite par génération PDF, même à plusieurs répétitions.
**FX-05 :** Si tarifs des services ont des devises différentes, interdire addition avant conversion explicitement approuvée. La base métier reste une devise unique par facture dans V1. Vérifier `calculate_flat_rate` qui reçoit parfois `currency='USD'` indépendamment du plan.

## P0 — Sélection tarifaire déterministe et honnête

**TA-01 :** `tariff_plan_for` et tous chemins similaires doivent refuser un plan `active=0` pour de nouvelles factures et refuser `confirmed=0` lorsque confirmation requise. Un même service, type, période, devise ne doit avoir qu'un tarif utilisable déterministe.
**TA-02 :** **Ne pas revenir silencieusement** à un tarif antérieur après désactivation d'un tarif plus récent : définir un comportement métier non ambigu et conservateur (par défaut : dernier tarif applicable désactivé → facturation bloquée jusqu'à nouvelle configuration explicitement approuvée). Expliquer comment gérer une date de fin et les anciennes périodes.
**TA-03 :** Assurer la cohérence entre `current_tariffs`, liste paramètres, `billing_configuration`, calcul, preview, émission, facture, JSON/CSV. Éviter les tarifs anciens affichés comme actifs. Les profils fiscaux ont une sélection active/confirmée datée propre à l'eau, sans copier la taxe électrique.
**TA-04 :** Validation serveur des montants/taux et identifiants service; devise active et identique sur la facture; aucune valeur d'eau réelle, taxe ou exemption inventée. Les fixtures de test portent le mot TEST et ne sont jamais injectées en prod.

## P0 — Facture nouvelle obligatoirement eau + électricité

**BI-01 :** Toutes les nouvelles factures sont **un seul document** contenant exactement deux services `ELECTRICITY` (KWH) et `WATER` (M3), aucune option mono-service, y compris tous clients historiques. Deux sections dans preview, détail, PDF et exports.
**BI-02 :** `water_m3 = 0` constitue une quantité valide, obligatoire, visible avec `0,00` de consommation. Si tarifs/fiscalité de l'eau non confirmés : enregistrer quantité 0, sous-total 0, montant 0, **prix inconnu « — »**, ne pas inventer prix 0 ou taux de TVA 0 ou exonération; absence de frais fixes d'eau tant que SEL ne les a pas validés. Si eau > 0 : exige tarif actif confirmé, profil fiscal actif confirmé, date applicable, source SEL et devise cohérente.
**BI-03 :** Eau à 0 et électricité à 0 : deux lignes et totaux nuls (hors frais dûment confirmés); eau >0 / électricité 0, eau 0 / électricité >0, toutes deux >0 : tester. Consommation absente ≠ quantité 0; valeurs négatives, NaN, précision excessive rejetées.
**BI-04 :** Ne pas préremplir un zéro susceptible d'être confirmé involontairement sans rappel efficace. Mécanisme acceptable : saisie explicite (champ vide) OU acquittement visible d'une valeur 0 préremplie; validation serveur anti-oubli lorsque 0 est prérempli. Choisir UX minimale adaptée à SEL, sans forcer un deuxième écran inutile.

## P0 — Données, historique, état de raccordement

**DATA-01 :** **Ne pas déduire** du fait que l'eau apparaît sur toutes les factures que tous les clients sont raccordés physiquement. Retirer les `UPDATE clients SET water_enabled=1` et triggers coercitifs universels si leur seule fonction est d'obtenir l'affichage; conserver `water_enabled` (ou attribut historique) comme information métier distincte si sa sémantique précédente est connue. La composition obligatoire est indépendante de ce flag. Ne pas effacer un ancien 0; migration idempotente.
**DATA-02 :** Si la branche seule a déjà transformé des clients de test, corriger sa migration. Si des bases réelles avaient déjà subi la transformation, **ne pas inventer** un état de raccordement original impossible à récupérer; fournir analyse de récupération à partir de sauvegardes pré-migration. Aucune réécriture massive de données clients sans justification.
**DATA-03 :** Les snapshots des factures émises restent **immutables**, incluant clients, montants, taxes, numéros, paiements et annulations. Documents antérieurs mono-service restent mono-service historiquement; ne pas fabriquer une ancienne eau à 0 rétroactivement. Préserver lecture/export/impression adaptés à la version du document. Si ancien titre semble anachronique, prouver ce qu'avait été émis et choisir fidélité au document sans réécrire le snapshot.
**DATA-04 :** Sauvegarde SQLite cohérente, tests sur DB ancienne et mise à niveau additive; exports `sel-facturation-portable` version 1 restent lisibles et justes; corrections paiements append-only, pas de duplication du total à régler sur plusieurs services.
**DATA-05 :** Confirmation de facture protégée contre sessions/brouillons obsolètes : jamais émission mono-service par reprise d'un ancien preview; vérifier catégorie client/service/paramètres et conserver les tarifs réellement approuvés lors de la validation, avec transaction `BEGIN IMMEDIATE` et contrôle facture active.
**DATA-06 :** Le compte de paiements reste unique par facture et le solde est unique. Répartitions par service uniquement analytiques tant que SEL n'a pas demandé d'affectation comptable des règlements.

## P1 — Qualité documentaire et cohérence

**DOC-01 :** A4 lisible imprimé, texte et totaux sans débordement, 0 m³ explicite, prix inconnu « — », détail eau/électricité, taxes séparées si confirmées, statut des règlements exact.
**DOC-02 :** QR interne avec numéro/date/devise/total **source**, aucun renseignement privé, ni fausse certification DGI. Décrire clairement dans le document que ce QR ne certifie pas fiscalement le PDF. Si variante CDF, éviter toute ambiguïté entre montant source du QR et montants affichés.
**DOC-03 :** Vérifier le parcours et la terminologie : « eau + électricité », compteurs optionnels seulement si effectivement connus, aucun décompte ancien renommé rétroactivement sans justification. Si seuil de page multipage, titre/numéro/résumé correct et pas de section coupée incohérente.
**DOC-04 :** Écran paramètres cohérent avec la date, statut, service, type de client et devise. Une simple mutation `confirmed=yes` n'est pas une preuve juridique; documenter la confirmation de SEL requise.

## P1 — Discipline du changement

**CH-01 :** Auditer les effets de bord sur `invoice_pdf.py`, `app.py`, `domain.py`, `db.py`, `config_service.py`, templates, JS, export CSV/JSON, paiements, versions/CI; tout changement doit être couvert par test.
**CH-02 :** Écarter les PR obsolètes #41/#46 seulement après comparaison de leurs différences et confirmation qu'aucune exigence indépendante n'est perdue; pas de fermeture/fusion automatique.
**CH-03 :** SafeUpdate non signé Windows Authenticode garde **Ed25519 obligatoire**; aucune porte dérobée, aucune clé privée dans dépôt, aucun contournement SmartScreen. Ne pas confondre remise d'un installeur de test et mise à jour in-app opérationnelle.
