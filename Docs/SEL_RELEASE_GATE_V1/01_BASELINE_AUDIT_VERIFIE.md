# Baseline d'audit vérifiée — ne pas traiter comme verdict définitif

**Dépôt :** `jupi-z/SEL-Facturation-Desktop`. **Contrôle du 23/10/2026**. Codex doit réexaminer les nouvelles modifications avant correction.

## Ce qui existe dans la PR #47 à `df3be22`
- Version déclarée `0.3.32`. Parcours eau et électricité sur une facture; `0 m³` accepté; tarif de l'eau positif bloqué sans tarif + fiscalité confirmés; PDF A4, QR documentaire interne, SQLite et export portable.
- Des tests applicatifs de PDF zéro eau et migrations existent, et la dernière CI Windows de cette PR à https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/39974626585 est **success**, EXE/MSI/Burn compilés; ne prouve pas une impression ni une installation sur le PC SEL.
- `main` ne contient pas cette PR; la dernière release téléchargeable vérifiée était `v0.3.29`. Brouillon `v0.3.31` sans asset ; ne pas le présenter comme livré.

## Défauts à reproduire (ne pas supposer le résultat avant tests)

### FX/PDF CDF — `app.py` route `/invoices/<iid>/pdf`
Le code multiplie une première fois `calc['total']` par le taux; sérialise `inv['calc_snapshot']`; modifie **ensuite** des lignes `calc['services']` (montants, sans systématiquement changer prix unitaires et devises), puis remultiplie `calc['total']` en mémoire, **sans re-sérialiser**. **Important :** le montant doublement multiplié en mémoire n'est pas nécessairement celui du PDF, car `render_invoice` désérialise le snapshot déjà enregistré dans `inv`. Le défaut à prouver concerne donc particulièrement la **désynchronisation** de total, services, ligne de prix, devise et paiements dans le PDF CDF. Ne pas publier le chiffre de 61 376 992 CDF comme résultat PDF avéré sans test réel. Un bug peut être à la fois masqué et dangereux à de futures modifications.
- Fichier `invoice_pdf.py` : `render_invoice` réutilise des sous-totaux de services, leur `currency`, les montants des lignes et le total du snapshot; ce sont autant d'assertions à rapprocher.
- `tests/test_invoice_pdf_tariffs.py` teste surtout du texte statique / l'appel PDF direct, pas le trajet HTTP de conversion d'une facture combinée. Ajouter des tests end-to-end.

### Tarifs inactifs — `db.py:tariff_plan_for`
La requête filtre `customer_type`, `service_code`, `effective_from`, `confirmed` parfois, mais **pas** `active=1`. Le dernier tarif désactivé peut être choisi. `config_service.py:current_tariffs` a également une sélection sans `active=1`. Corriger sémantique et écran; ne pas masquer un tarif désactivé par un fallback silencieux sans politique explicite.

### Couplage facturation et raccordement — `db.py:initialize`
La migration `UPDATE clients SET water_enabled=1` réécrit les anciennes valeurs et des triggers `clients_water_required_*` imposent toujours 1. L'obligation de présenter une ligne eau **n'établit pas** que le client est physiquement raccordé. Préserver l'état déclaré antérieurement lorsqu'il avait du sens; découpler l'affichage de l'état de raccordement. Si valeur déjà écrasée dans un test, ne pas inventer sa restitution.

### Zéro eau automatique — `templates/new_invoice.html`
`water_m3` est prérempli `0`. Risque de valider par inadvertance une consommation positive comme nulle; concevoir un dispositif clair de confirmation ou saisie volontaire, tout en satisfaisant l'obligation `0 m³` sur facture. Le contrôle doit être au serveur également.

### Documents anciens — `invoice_pdf.py`
Le rendu `document_template_version=2` peut présenter un titre « facture d’eau et d’électricité » sans ligne d'eau selon snapshot. Identifier l'origine et assurer **la fidélité historique** avant tout changement de titre. Aucun recalcul des factures passées.

### Cohérence des données et de l'affichage
- `app.py` `combine_invoice_calculation` fabrique pour eau 0 non configurée un service sans `unit_price` et sans `tax_profile_code` (correct en principe), mais vérifier que PDF et aperçu affichent **« — »** pour tarif inconnu, pas « 0 USD/m³ » ou « TVA 0 % » qui simulerait un régime.
- `invoice_pdf.py:internal_qr_payload` contient référence, date, devise et montant du **document source**, sans données personnelles, distinct du QR DGI. Variante CDF doit clairement identifier la devise source du QR et ne pas tromper l'usager sur un total converti.
- Les données de `water_enabled` précédentes ne doivent pas être écrasées; état commercial et présence documentaire sont distincts.

### Update/packaging
- `safeupdate_keys.py` sur `main` est vide (clés publiques production non provisionnées), `updates/latest.json` ancien; ne pas rendre l'update automatique « opérationnel production » en supprimant les contrôles de confiance. Une recette d'installation manuelle est possible; l'auto-update reste bloqué tant que la chaîne Ed25519 n'est pas provisionnée et validée.
- Le PDF, la saisie, les paiements, les exports et l'installateur doivent être testés contre les données historiques. Aucune clé privée ou données SEL réelles dans les tests.

## Interprétation du statut
**Success CI = build/tests existants réussis.** Ce n'est ni recette matérielle, ni preuve d'absence de bug financier. Toute divergence ci-dessus doit être classée selon sa preuve : reproduite par test, analyse statique certaine, risque à reproduire, non applicable après travaux concurrents. Les fichiers source resteront l'autorité, pas le rapport antérieur.
