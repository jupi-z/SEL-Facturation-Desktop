# Rapport — facturation combinée et CRUD sûr

## Correctif de parcours eau — version 0.3.31

- Cause reproductible : le formulaire affichait la consommation d’eau pour tous les abonnés. Un `0` conservé par le brouillon après changement de client était envoyé au serveur, qui le rejetait quand l’abonné n’était pas raccordé.
- Le champ eau est désormais masqué et désactivé pour l’abonné courant sans raccordement, puis réinitialisé au changement d’abonné. La fiche indique comment activer le service et où vérifier les paramètres eau.
- Défense serveur : un `0` résiduel est sans effet pour un abonné non raccordé; une quantité positive ou invalide est toujours refusée. Pour un abonné raccordé, `0 m³` reste obligatoire à saisir et apparaît sur la facture combinée.
- Aucun tarif, taux ou profil fiscal d’eau n’est ajouté. Le tarif et le profil fiscal doivent toujours être explicitement saisis et confirmés par SEL.
- Migration SQLite : aucune. Les snapshots des factures historiques et les exports restent inchangés.
- Version de test : `0.3.31`; l’installateur demeure à construire/valider pour cette version. Aucune release ni déploiement n’est effectué.

Version de travail : **0.3.30**, branche `monarque/water-electricity-crud`.

## Portée livrée dans cette branche

- Paramètres tarifaires et fiscaux explicitement rattachés à l’électricité ou à l’eau, avec période d’effet, source et état de confirmation. La migration classe les réglages existants comme électricité; elle ne crée aucun tarif ni régime fiscal d’eau.
- Option de raccordement eau sur la fiche abonné, inactive par défaut pour les dossiers existants. Une facture d’un abonné raccordé exige une saisie d’eau en m³, y compris `0`.
- Calculs financiers `Decimal`, ligne de service et taxation indépendantes, même devise obligatoire, total commun. Le snapshot conserve les clés historiques et les nouveaux éléments `services`; les anciens documents ne sont pas recalculés.
- Prévisualisation, émission, fiche de facture, PDF A4, variante CDF, historique et exports adaptés. Les documents non-eau continuent à suivre leur parcours historique.
- CRUD des référentiels avec vérification transactionnelle des dépendances; suppression physique refusée lorsqu’un historique l’utilise, avec désactivation/archivage proposé.
- Correction de paiements par écritures append-only datées et motivées. L’encaissement net s’applique aux soldes, rapports et PDF. La facture ne peut être annulée qu’après neutralisation intégrale des paiements; ni paiement ni facture ne sont effacés.
- Export JSON `sel-facturation-portable` maintenu en `schema_version: 1`, avec services et contre-écritures comme données additionnelles. Les exports CSV exposent les quantités eau, montants par service et corrections de paiement.

## Protection des données et compatibilité

Migrations SQLite additives/idempotentes; aucun effacement de données SEL, aucune mise à jour rétroactive de facture et aucun changement de pile. Les tests utilisent exclusivement des bases temporaires et données synthétiques. Les factures et paiements historiques restent lisibles; les nouvelles contre-écritures protègent également leur propre historique contre UPDATE/DELETE.

## Validation technique

- Suite complète : **284 tests réussis** avec les assertions d’export, d’immutabilité et d’idempotence des migrations incluses (`py -3.12 -m pytest -q`, 122,43 s).
- Le scénario ciblé paiements/corrections, y compris dépassement refusé, invariance de l’écriture d’origine, annulation finale et exports, est couvert par `tests/test_crud_management.py`.
- Les tests PDF contrôlent les sections eau/électricité, la ligne de 0 m³ et le taux de conversion; un rendu PNG synthétique de contrôle a été généré localement. Build Windows 11/Python 3.12 : EXE gelé et smoke test réussis, MSI WiX et installeur compilés.
- Le bootstrapper `dist/installer/SEL-Facturation-Setup-0.3.30.exe` (264 835 549 octets) embarque le runtime WebView2 autonome x64 (214 823 632 octets, signature Microsoft `Valid`), contrairement au bootstrapper WebView2 d’environ 2 Mio qui nécessite Internet. SHA-256 de l’installeur : `4CA8B75C49BE2A5BE3284D92CA0CB68530DB1BDB06681448157713F26FF11B05`. Le paquet SEL est `NotSigned` (signature Authenticode de l’éditeur non encore disponible).

## Limites et décisions opérationnelles

Le mécanisme d’administration est prêt à recevoir les références eau, mais la facturation réelle de l’eau demeure bloquée jusqu’à confirmation par SEL du prix par m³, de la devise, des catégories et dates d’effet, ainsi que du régime et de l’assiette fiscale applicables. Aucun exemple de test n’est utilisé comme réglage réel.

Le paquet local non signé reste un artefact de test. La recette de l’installation et du lancement sur le poste matériel de SEL n’est pas réalisée; le runtime est embarqué et contrôlé, mais ce contrôle de build ne remplace pas un essai sur un poste vierge. La signature Authenticode, une release publique et le déploiement chez SEL ne sont pas réalisés par cette branche. Aucune fusion dans `main` ni publication n’est autorisée sans validation SEL.
