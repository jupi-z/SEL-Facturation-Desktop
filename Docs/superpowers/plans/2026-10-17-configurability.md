# Configuration métier SEL — Plan d’implémentation

> **Pour les agents :** utiliser `superpowers:executing-plans` pour exécuter ce plan tâche par tâche.

**Objectif :** centraliser les règles métier configurables afin que SEL puisse modifier ses types d’abonnés, tarifs, taxes, devises, numérotation et politiques opérationnelles sans modifier le code, tout en protégeant les factures historiques.

**Architecture :** conserver Python/Flask/SQLite. Ajouter une couche de configuration typée au-dessus des tables existantes, avec migrations additives et compatibilité avec `settings`, `tariff_plans`, `exchange_rates` et l’export JSON v1. Les règles effectives seront versionnées par date et copiées dans les snapshots de facture.

**Technologies :** Python 3.12, Flask, SQLite, Decimal, pytest, interface native pywebview.

**Spécification :** analyse de configurabilité validée dans la conversation du 17/10/2026.

## Contraintes globales

- Ne pas remplacer la pile Python/Flask/SQLite.
- Migration SQLite additive uniquement.
- Ne jamais recalculer ni modifier les snapshots de factures émises.
- Préserver `sel-facturation-portable` `schema_version: 1`.
- USD principal, CDF variante, taux initial 1 USD = 2360 CDF.
- Résidentiel 0,095 USD/kWh HT, commercial 0,12 USD/kWh HT, TVA 16 %, taxe locale désactivée.
- Les compteurs restent masqués tant que le mode compteur n’est pas activé.
- Ne jamais ajouter de certification, QR ou identifiant fiscal fictif.

## Points de revue

- Un nouveau type d’abonné doit fonctionner sans modification de `app.py`.
- Une modification de tarif doit laisser une facture antérieure inchangée.
- Une modification du taux CDF ne doit pas modifier les factures USD déjà émises.
- Une section de paramètres ne doit pas effacer une autre section.
- Une migration ou mise à jour doit conserver les données et l’export JSON v1.

### Tâche 1 : Modèle de configuration et compatibilité additive

**Fichiers :**
- Modifier : `db.py`
- Créer : `config_service.py`
- Tester : `tests/test_config_service.py`, `tests/test_tariff_persistence.py`

- [ ] Ajouter les tables `customer_types`, `currency_catalog`, `tax_profiles`, `billing_policy` et `configuration_history` sans supprimer les tables existantes.
- [ ] Seeder deux types actifs (`RESIDENTIAL`, `COMMERCIAL`), USD/CDF et le profil TVA SEL.
- [ ] Ajouter des fonctions typées de lecture et d’écriture avec validation et dates d’effet.
- [ ] Utiliser `tariff_plans` et `exchange_rates` comme sources historiques compatibles, puis exposer une lecture unifiée.
- [ ] Écrire les tests de migration, de nouveau type et de conservation des anciennes lignes.

### Tâche 2 : Moteur de calcul et snapshots

**Fichiers :**
- Modifier : `domain.py`, `app.py`
- Tester : `tests/test_consumption_billing.py`, `tests/test_consumption_http.py`, `tests/test_invoice_preview.py`

- [ ] Remplacer la validation codée en dur des types par une validation provenant de la configuration.
- [ ] Résoudre tarif, devise et profil fiscal à la date de la période facturée.
- [ ] Ajouter dans `calc_snapshot` les identifiants/version des règles appliquées.
- [ ] Garantir que la confirmation utilise exactement la configuration de la prévisualisation ou refuse une prévisualisation devenue obsolète.
- [ ] Vérifier les cas 0, 1, 100, 200 kWh et changement de tarif entre deux périodes.

### Tâche 3 : Interface d’administration

**Fichiers :**
- Modifier : `app.py`, `templates/settings.html`, `templates/clients.html`, `static/app.css`, `static/brand.css`
- Tester : `tests/test_ui_tariff_rules.py`, `tests/test_form_accessibility.py`, `tests/test_ui_modals.py`

- [ ] Afficher les types et tarifs depuis la base, sans tarifs codés en dur dans le template.
- [ ] Permettre l’ajout, l’activation et la désactivation d’un type futur.
- [ ] Permettre la création d’un tarif avec date d’effet et confirmation explicite.
- [ ] Afficher le taux de change courant et son historique.
- [ ] Maintenir les champs compteur/coefficient masqués par feature flag.
- [ ] Ajouter une validation visible et accessible avant toute modification financière.

### Tâche 4 : Politiques opérationnelles configurables

**Fichiers :**
- Modifier : `app.py`, `db.py`, `desktop.py`, `update_client.py`
- Tester : `tests/test_reporting.py`, `tests/test_storage_pdf.py`, `tests/test_update_http.py`, `tests/test_update_policy.py`

- [ ] Centraliser numérotation, méthodes de paiement, échéance, rétention des sauvegardes et rétention PDF.
- [ ] Garder les limites de sécurité, signatures, HTTPS, downgrade et chemins de données hors de la configuration utilisateur.
- [ ] Ajouter une lecture de politique de mise à jour stable sans permettre à l’utilisateur de remplacer la confiance de signature.

### Tâche 5 : Export, documentation et vérification

**Fichiers :**
- Modifier : `README.md`, `Docs/CODEX_HANDOFF.md`
- Tester : suite complète pytest, build Windows si l’environnement le permet

- [ ] Documenter les paramètres administrables et ceux verrouillés.
- [ ] Vérifier que l’export portable v1 reste inchangé et complet.
- [ ] Exécuter la suite complète et produire un rapport des limites restant non testées sur le poste SEL.

