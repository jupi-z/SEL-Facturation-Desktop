# Facturation eau + électricité et CRUD sûr — Plan d’implémentation

> **Pour les agents de code :** plan à exécuter par tâches; appliquer TDD et valider chaque tâche avant de passer à la suivante.

**Objectif :** ajouter la facturation combinée eau/électricité et compléter les opérations CRUD métier sans perdre ni réécrire l’historique financier.

**Architecture :** étendre les référentiels existants par service, ajouter une activation eau par abonné et composer des snapshots `services` rétrocompatibles. Garder le calcul pur dans `domain.py`, les migrations dans `db.py`, les routes dans `app.py`, et rendre chaque ancienne facture selon son template enregistré.

**Pile :** Python 3.12, Flask, SQLite, Jinja, ReportLab, pytest; aucune dépendance réseau ou nouvelle pile.

**Spécification :** `Docs/superpowers/specs/2026-10-23-eau-electricite-crud-design.md`, avec règles métier complémentaires dans `Docs/SEL_BILLING_COMBINED_V1/01_SPECIFICATION_NORMATIVE.md` et les décisions non confirmées dans `03_DECISIONS_A_CONFIRMER_SEL.md`.

## Contraintes globales

- Migrations SQLite additives, répétables, sauvegarde préalable; ne jamais écraser les snapshots ou paiements historiques.
- Pas de tarif, taxe, exemption ou assiette eau par défaut; l’émission réelle est bloquée tant que SEL n’a pas confirmé les réglages.
- Devise de facture unique; utiliser `Decimal` et `ROUND_HALF_UP`; pas de calculs financiers en `float`.
- Export JSON `sel-facturation-portable` v1 maintenu; nouvelles clés documentées et optionnelles.
- Factures émises immuables; paiements corrigés par écritures traçables; pas de suppression en cascade.
- Application locale Windows existante, aucun endpoint distant ou dépendance Internet ajoutée.
- Branche dédiée, PR et CI Windows verte; ne pas fusionner ou publier de release sans accord explicite.

## Risques à couvrir en tests

1. Eau activée mais tarif/profil fiscal manquant : aucun aperçu émissible ni snapshot créé.
2. Eau activée à 0 m³ : ligne visible, bases/taxes/montants à zéro, sans masquer le prix configuré.
3. Eau désactivée : pas de ligne ni de montant inventé; le client reste facturable en électricité.
4. Deux services avec devise ou taxe différente : rejet si devise incohérente et bases séparées par service.
5. Modification d’un barème après émission : les anciens snapshots et PDF restent identiques.
6. Suppression d’un référentiel utilisé ou d’un client facturé : refus de hard-delete, proposer désactivation/archivage.
7. Correction paiement : l’écriture initiale reste visible, le montant net/solde tient compte de la correction.
8. Snapshot ancien électrique sans `services` : rendu, détails et export continuent à fonctionner.

---

### Task 1 — Migrations additives et règles de service

**Fichiers :** `db.py`, `config_service.py`, nouveaux tests `tests/test_service_schema.py`, `tests/test_config_service.py`.

**Interfaces :** `db.tariff_plan_for(c, customer_type, on_date, service_code="ELECTRICITY")`; utilitaires de configuration qui retournent uniquement les plans et taxes actifs à la date donnée.

- [ ] Écrire des tests de migration depuis la base actuelle : ancienne grille électrique intacte; pas de plan/taxe eau créé; nouvelle colonne/service et métadonnées de validation disponibles.
- [ ] Exécuter ces tests et vérifier les échecs attendus avant migration.
- [ ] Ajouter `service_code` aux plans/profils fiscaux avec valeur électrique pour les anciennes lignes; ajouter les champs de validation/source nécessaires et les indexes.
- [ ] Étendre la recherche de tarif avec filtrage strict service/type/date; créer les helpers de règles fiscales datées.
- [ ] Vérifier migration répétée, ancienne base, tarification électrique et tests de configuration.

### Task 2 — Calculs unitaires et composition de services

**Fichiers :** `domain.py`, `tests/test_service_billing.py`, `tests/test_consumption_billing.py`.

**Interfaces :** `calculate_service(*, service_code, quantity, unit, unit_price, taxes, currency) -> dict`; `combine_services(service_calculations, currency) -> dict`.

- [ ] Tester kWh et m³, arrondis, zéro, valeurs négatives/NaN, service/unité incohérents, taxe explicite à 0 et absence de règle de taxe.
- [ ] Constater les échecs de ces nouveaux tests avant implémentation.
- [ ] Implémenter calcul unitaire `Decimal`, taxes indépendantes (assiette par service), sous-total et total; ajouter un adaptateur qui conserve les champs électriques historiques.
- [ ] Tester les valeurs électriques de référence et l’exemple eau synthétique réservé aux tests (`10 m³ × 0,40 USD = 4,00 USD`, aucune fixture copiée en base réelle).
- [ ] Exécuter tous les tests du moteur.

### Task 3 — Fiche abonné et configuration eau avec CRUD sûr

**Fichiers :** `db.py`, `app.py`, `templates/client_edit.html`, `templates/clients.html`, `templates/settings.html`, `tests/test_crud_management.py`, `tests/test_tariff_persistence.py`, `tests/test_ui_tariff_rules.py`.

**Interfaces :** données client eau `water_enabled`; plans et profils fiscaux édités via les endpoints existants de Paramètres, avec type de service explicite et référence/source de validation SEL.

- [ ] Tester activation/désactivation eau client, persistance après modification, validation des paramètres eau et blocage sans validation.
- [ ] Ajouter champs de service eau à la création/édition client et vérifier les brouillons/formulaires.
- [ ] Ajouter création/modification/désactivation/suppression de plans, règles fiscales, types, devises et moyens de paiement; suppression physique refusée si référencée, historique de configuration préservé.
- [ ] Ajouter la configuration de l’eau sans valeurs par défaut et une confirmation explicite documentée/source SEL.
- [ ] Ajouter/compléter les actions CRUD d’archivage, restauration et suppression dans les vues de gestion; garder les abonnés facturés archivables mais non supprimables.
- [ ] Exécuter tests CRUD/config/UI ciblés.

### Task 4 — Prévisualisation, confirmation et snapshots combinés

**Fichiers :** `app.py`, `templates/new_invoice.html`, `templates/invoice_preview.html`, `templates/invoice_detail.html`, `tests/test_consumption_http.py`, `tests/test_invoice_preview.py`.

**Interfaces :** nouveau snapshot `document_schema_version=2`, `services=[{code,mode,quantity,unit,unit_price,taxes,subtotal,total}]`; champs électriques historiques restent au niveau racine pour consommateurs existants.

- [ ] Tester facture électricité seule (client sans eau), eau active à 0, eau positive, absence de tarif/taxes, double émission, correction de brouillon et stabilité du tarif aperçu.
- [ ] Observer les échecs attendus.
- [ ] Ajouter saisie m³, sélection claire d’abonné/service, aperçu des deux détails et totaux réconciliés; garder le parcours électrique historique.
- [ ] Revalider au POST de confirmation, recalculer à partir des valeurs de snapshot du brouillon, numéro unique, transaction atomique et contrôle du doublon.
- [ ] Enregistrer snapshots combinés immuables et tester les anciennes factures inchangées.

### Task 5 — Cycle de vie CRUD des factures et paiements

**Fichiers :** `db.py`, `app.py`, `templates/invoice_detail.html`, `tests/test_invoice_actions.py`, `tests/test_crud_management.py`.

**Interfaces :** action de correction de paiement append-only; facture émise corrigée par annulation et nouvelle émission, sans UPDATE/DELETE des snapshots.

- [ ] Tester annulation/correction d’un paiement, raison obligatoire, solde net, annulation de facture payée après correction, et maintien des lignes initiales.
- [ ] Ajouter table append-only de contre-écritures, agrégation net-payé et UI de correction accessible dans le détail facture.
- [ ] Ajouter un flux « corriger/réémettre » pour les factures non réglées; pour une facture réglée, exiger correction complète des paiements d’abord.
- [ ] Ajouter boutons d’actions destructive avec confirmation/raison; préserver les triggers d’immutabilité et les historiques.
- [ ] Exécuter tests facture/paiements/CRUD.

### Task 6 — PDF A4, historiques, tableaux de bord et exports

**Fichiers :** `invoice_pdf.py`, `app.py`, templates factures/dashboard, tests PDF/export/reporting.

- [ ] Tester extraction PDF des sections distinctes, ligne eau à zéro, détail prix et taxe par service, total/paiements, compatibilité snapshots v1, devise et annulation.
- [ ] Implémenter deux tableaux vectoriels séparés, total commun et QR interne non fiscal, multipage et texte utilisateur échappé.
- [ ] Adapter recherche/détails/dashboard/CSV/portable JSON pour consulter les montants de chaque service; maintenir schéma d’export v1 et anciens champs.
- [ ] Produire des PDF de test uniquement avec données synthétiques et vérifier pages/rendu visuel.
- [ ] Exécuter tests PDF, export et reporting.

### Task 7 — Audit CRUD final, version, vérification et PR

**Fichiers :** tests métier/UI, `README.md`, `Docs/REVIEW_REPORT.md`, version/changelog si requis.

- [ ] Vérifier matrice CRUD pour chaque objet métier : créer, consulter, modifier, supprimer/désactiver/archiver selon dépendances, rétablir lorsque applicable.
- [ ] Exécuter `py -3.12 -m pytest -q`, `python -m compileall -q .`, `git diff --check` et build local Windows si les dépendances/outils sont disponibles.
- [ ] Vérifier migrations sur base ancienne et nouvelle, sauvegarde/intégrité SQLite, exports, PDF synthétiques et smoke test EXE.
- [ ] Documenter le tarif/taxe eau toujours à fournir, la recette matérielle non exécutée et les limites.
- [ ] Bumper la version seulement après que les fonctionnalités et les tests de cette branche sont stabilisés; créer PR avec rapport et preuves. Pas de fusion/release.

## Interfaces entre tâches

- Tâche 1 produit les lookups `service_code` utilisés par Tâches 2–4; unité historique `KWH` mappe vers `ELECTRICITY`.
- Tâche 2 produit les dicts de service combinables consommés par Tâches 4 et 6; les clés historiques de l’électricité restent lisibles par Tâches 5–6.
- Tâche 3 fournit l’éligibilité et la configuration nécessaires à Tâche 4; sans validation eau, le chemin combiné émissible doit échouer proprement.
- Tâche 5 dépend du total root-level immuable produit par Tâche 4; les contre-écritures changent le solde seulement, jamais le QR ou le total original.
- Tâche 6 lit les deux formats de snapshot sans modifier la base.

## Revue critique

- Prix/taxe eau non confirmés : bloquer l’émission, pas seulement afficher une alerte.
- Zéro m³ : distinguer service actif à consommation nulle de service absent.
- Règles de suppression : contrôler dépendances en base au moment de l’action, même si le bouton UI est masqué.
- Numérotation/doublon : revalider en transaction, préserver l’index unique et utiliser une nouvelle référence pour une réémission.
- Anciennes données : la migration doit être répétable et ne doit jamais mettre `services` dans les anciens snapshots.
