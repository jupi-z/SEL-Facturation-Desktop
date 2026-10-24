# Rapport d’exécution — SEL Release Gate V1

Date de travail : 24 octobre 2026  
Branche : `monarque/water-electricity-crud` (PR #47)  
Base intégrée : `main` via le commit de merge `ff56862b563839119f5f1532bd0ea36840a0f9a3`  
Version applicative conservée : `0.3.32` (aucune release ni publication demandée)

## État d’avancement

| Phase | État | Critère observé |
|---|---|---|
| 0. Spécifications, issue #49, état de PR #47 et `main` | Terminé | Documents normatifs de release-gate lus; différences de `main` intégrées sans écraser le travail de PR #47. |
| 1. Audit du code, des snapshots et des migrations | Terminé | Défauts reproductibles localisés dans les lookups tarifaires, l’état de raccordement, la confirmation de facture et le PDF CDF. |
| 2. Corrections métier, SQLite, facturation et compatibilité | Terminé en local | Migration additive, aucune réécriture des snapshots; les tests dédiés passent. |
| 3. Calculs, tests de régression, PDF/QR et exports | Terminé localement | Trois vrais PDFs issus des routes générés, extraits et rendus en image; suite complète : 296 tests réussis. |
| 4. Build/CI Windows du commit final | En attente du push du commit candidat | Le build local d’installateur ne remplace pas le runner Windows; aucun résultat final n’est revendiqué avant CI. |
| 5. Essai sur poste SEL et décision de livraison | Non réalisé | Aucun accès au matériel SEL; pilote non validé sur poste cible. |

Progression globale vérifiable : **en cours — tests et CI finale non encore clôturés**. Les phases 4 et 5 sont des critères obligatoires et ne sont pas comptées comme réussies.

## Corrections réalisées

- **Tarifs et taxes** : l’émission exige maintenant le dernier tarif et le dernier profil fiscal applicable, actifs et confirmés. Un tarif/profil plus récent inactif ou non confirmé bloque la facturation; le moteur ne retombe plus sur une ancienne valeur. Les vues historiques peuvent encore consulter un tarif inactif sans le rendre facturable.
- **Un seul document, deux services** : toute nouvelle facture conserve les lignes Électricité (kWh) et Eau (m³), y compris exactement `0 m³`. Le zéro requiert un accusé de confirmation explicite, contrôlé côté serveur; sans tarif eau validé, la ligne reste à montant nul, sans prix ni taxe inventés.
- **Raccordement physique distinct** : ajout d’un statut client `UNKNOWN / CONNECTED / DISCONNECTED`. L’eau reste présente sur toutes les factures; une quantité positive est bloquée si le raccordement est inconnu ou déclaré déconnecté. Les anciennes valeurs potentiellement forcées par les déclencheurs de l’ancienne version deviennent `UNKNOWN`, sans tentative de reconstitution. Les déclencheurs coercitifs et le backfill universel sont supprimés.
- **Confirmation transactionnelle** : les consommations et paramètres sont recalculés au clic d’émission. Toute différence avec l’aperçu exige une nouvelle prévisualisation et ne consomme ni numéro ni séquence.
- **USD/CDF** : la variante PDF sélectionne le taux SEL applicable à la date d’émission, refuse un taux fourni librement dans l’URL et convertit une seule copie d’affichage. Les lignes, prix unitaires, sous-totaux, bases et taxes, totaux, paiements nets et solde sont cohérents; quantités inchangées. Le snapshot source en USD reste byte-à-byte identique. Le QR interne continue de référencer le document source et ne prétend à aucune validation fiscale.
- **Historique PDF** : un ancien snapshot électrique seul reste présenté comme facture électrique; les documents archivés ne sont pas réécrits. Le QR interne et le bloc de règlement sont réunis dans une composition A4 d’une page.
- **Édition client** : l’état physique de raccordement est modifiable séparément de l’obligation d’inclure les deux services. La valeur binaire historique `water_enabled` est conservée si le nouveau statut reste inconnu.

## Preuves PDF produites par les routes

La commande reproductible est `py -3.12 tools/generate_release_gate_pdf_evidence.py`. Elle utilise un répertoire temporaire SQLite et deux abonnés synthétiques; elle ne lit ni n’écrit la base SEL de l’utilisateur. Chaque PDF a été obtenu par `GET /invoices/<id>/pdf`, extrait avec `pypdf` et rendu en PNG avec Poppler. Les trois fichiers ont une page A4 et le manifeste inclut leur SHA-256 et l’empreinte du texte extrait.

| Cas réellement extrait | Valeurs contrôlées | Snapshot après PDF |
|---|---|---|
| USD, eau à 0 sans tarif eau | Ligne eau `0 M3`, `0,00 USD`, sans prix d’eau ni taxe eau fictifs; total électrique/document `11,02 USD`. | Identique |
| CDF, eau à 0 sans tarif eau | Électricité HT `22 420,00 CDF`, TVA `3 587,20 CDF`, total `26 007,20 CDF`; ligne eau `0 M3`, `0,00 CDF`. | Identique |
| CDF, eau `2 M3`, paiement partiel puis correction | Électricité HT `22 420,00`; eau HT `1 888,00`; total `27 895,20`; encaissé net `2 124,00`; solde `25 771,20 CDF`. | Identique |

Fichiers de preuve : `Docs/SEL_RELEASE_GATE_V1/evidence/` (`.pdf`, `.png`, `manifest.json`). Les données nominatives des PDFs sont exclusivement synthétiques.

## Vérifications

| Contrôle | Résultat actuellement établi | Commande / preuve |
|---|---|---|
| Suite complète, y compris calculs, migrations, exports, QR et SafeUpdate | **296 passés, 0 échoué** en 140,96 s | `py -3.12 -m pytest -q` |
| Vérification de patch | Passe au dernier contrôle | `git diff --check` |
| PDF réel via route / extraction / rendu visuel | Passe pour 3 cas, une page chacun | Script de preuve ci-dessus; empreintes dans `evidence/manifest.json`. |
| Tests adversariaux Ed25519/SHA-256 et transactions SafeUpdate | Inclus dans la suite complète locale : réussis | `py -3.12 -m pytest -q`; aucun mécanisme de vérification n’a été désactivé. L’exécution indépendante du job Windows reste à vérifier en CI. |
| Windows build, packaging smoke, MSI/Burn autonome et CI sur SHA final | En attente | Workflow GitHub Actions `Build Windows Desktop`; aucun installateur n’est publié par cette intervention. |
| Installation/mise à niveau/impression sur poste SEL | Non testé | Matériel cible inaccessible : **pilote non validé sur poste cible**. |

## Préservation et limites

- Aucune base de données réelle n’a été ouverte ou modifiée. Les tests et preuves utilisent des répertoires temporaires.
- Les migrations SQLite sont additives et idempotentes; factures, paiements, numéros et snapshots existants ne sont pas modifiés par les nouveaux chemins.
- Une ancienne version a pu remplacer des états réels de raccordement par `water_enabled=1`. Ces états sont indéterminables à partir de cette valeur forcée; le nouveau statut reste `UNKNOWN` et doit être vérifié par SEL. Ne pas les deviner ni les reconstruire hors d’une sauvegarde fiable.
- Le tarif d’eau commercial, les profils de taxes/assiettes et leur date/source d’application doivent être confirmés par SEL. Les valeurs `0.40 USD/m³` et `0 %` utilisées dans les preuves ne sont que des fixtures synthétiques et ne constituent pas une proposition tarifaire.
- L’absence de clé de production Ed25519 et de manifeste signé signifie que la compatibilité SafeUpdate reste à valider avec les clés épinglées prévues; la vérification Ed25519 ne sera pas contournée.
- Les binaires resteront non signés Authenticode conformément à l’exigence actuelle; cela ne justifie aucun contournement Windows. Les contrôles SmartScreen et la recette matérielle restent à effectuer.

## Verdicts — provisoires jusqu’à la fin des contrôles

- Revue technique : **NO-GO provisoire** — suite complète et CI Windows sur le commit final non encore établies.
- Pilote Windows : **NO-GO** — **pilote non validé sur poste cible**.
- Livraison commerciale : **NO-GO** — confirmation écrite par SEL des tarifs/profils eau, recette sur poste cible et état SafeUpdate de production encore requis.

La PR #47 reste ouverte et ne sera ni fusionnée dans `main`, ni publiée en release, ni déployée chez SEL.
