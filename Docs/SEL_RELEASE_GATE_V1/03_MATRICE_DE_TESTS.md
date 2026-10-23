# Matrice de tests obligatoire — preuve de chaque critère

**Exécution :** chaque test nouveau doit d'abord échouer sur le code problématique, puis réussir sur le correctif (avec preuve du résultat). Tests sur DB synthétique avec `SEL_DATA_DIR` temporaire et clés de mise à jour **de test** si besoin. `pytest` seule ne remplace pas l'essai Windows réel.

## A. Conversion et argent (P0)
| ID | Scénario | Attendu |
|---|---|---|
| FX-01 | `100 kWh × 0.095 USD`, TVA électrique 16 %, `0 m³`, pas de charge eau, taux TEST `2360` | Source **11.02 USD**, PDF CDF **26007.20 CDF**, toutes lignes/subtotaux/devise réconciliables |
| FX-02 | `100 kWh` comme ci-dessus, `2 m³ × 0.40 USD`, fiscalité eau à 0 **fixture TEST uniquement** | Source **11.82 USD**, PDF CDF **27895.20 CDF**, sous-totaux service CDF et ligne unité correctement étiquetés |
| FX-03 | Variante CDF téléchargée via `GET /invoices/<id>/pdf?currency=CDF` | Extraire via pypdf les montants et devises réellement **dans le PDF généré par la route**, pas seulement appeler `render_invoice` avec un snapshot fabriqué |
| FX-04 | Répéter la requête PDF USD/CDF plusieurs fois | Snapshot SQLite original inchangé au byte près; numérotation et paiements inchangés |
| FX-05 | Paiement partiel + correction/reversal | `total_CDF – net_payments_CDF = balance_CDF` à la politique d'arrondi près, sans afficher montant > solde |
| FX-06 | Prix au m³ / kWh en PDF converti | Prix unitaire correctement converti et libellé par unité; si devises mixtes, indiquer explicitement et valider politique |
| FX-07 | Taxe locale, TVA, frais zéro/non zéro et arrondis limites | Sommes réconciliées par ligne/service/facture avec `Decimal` et règle d'arrondi stable |
| FX-08 | PDF variantes sans et avec eau, historiques v1/v2 | Numéro, devise et période exacts, aucune valeur CDF étiquetée USD |
| FX-09 | QR interne sur variante CDF | Le payload représente la référence du document source et son USD si source USD; le PDF porte une explication non ambiguë |

## B. Tarifs / référentiels (P0)
| ID | Scénario | Attendu |
|---|---|---|
| TA-01 | Dernier tarif eau applicable est `active=0` | Interdit à nouvelle facturation positive, aucun fallback silencieux non explicité |
| TA-02 | Dernier tarif électricité applicable est `active=0` | Interdit pour nouvelle émission; écran paramètres ne le présente pas comme disponible |
| TA-03 | Tarif eau confirmé=0 ou profil fiscal confirmé=0 | Eau positive refusée, eau zéro avec montant nul autorisée |
| TA-04 | Date future / période ancienne / type industriel | Sélection déterministe par date/type/service; aucune valeur inventée |
| TA-05 | Plusieurs tarifs pour même service/type/période | Refus ou résolution déterministe sans ambiguïté, traçable |
| TA-06 | Tarif eau actif mais devise incompatible avec facture | Eau positive refusée ou conversion explicite approuvée, jamais somme USD+CDF |
| TA-07 | TVA eau absente avec eau=0 | PDF affiche montant nul et tarif « — », **pas** faux « TVA eau 0 % » |
| TA-08 | Tarif/TVA modifiés après preview mais avant émission | Contrat du snapshot explicite, émission refusée si invalide, pas substitution silencieuse non validée |
| TA-09 | Désactivation historique de catégorie / plan / tax profile | Anciens PDF restent inchangés; nouvelles factures respectent l'état applicable |

## C. Service systématique et état métier (P0)
| ID | Scénario | Attendu |
|---|---|---|
| BI-01 | Nouveau client, `water_m3=0` | `services` exactement `ELECTRICITY,WATER`; PDF `0 m³` et `0.00` |
| BI-02 | Ancien client avec `water_enabled=0`, eau=0 | Même rendu combiné, **ancien flag conservé** (n'est pas une preuve de raccordement) |
| BI-03 | Eau=0, zéro tarif/profil eau configuré | Nouveau décompte possible avec ligne eau à 0 et tarif « — » |
| BI-04 | Eau positive sans tarif ou sans fiscalité | Refus avant émission/numérotation, message compréhensible |
| BI-05 | Eau positive avec tarif et fiscalité tests confirmés | Service eau calculé au m³, TVA et total exacts et snapshot émis |
| BI-06 | Électricité=0/eau=0 | Deux services conservés, facture/solde cohérents |
| BI-07 | Électricité=0/eau>0; électricité>0/eau>0 | Total commun exact, pas deux factures |
| BI-08 | Champ eau absent/négatif/non-fini/précision hors limite | Pas de document et pas de consommation présumée arbitrairement |
| BI-09 | Zéro prérempli + oubli agent | Saisie de 0 confirmée explicitement ou champ vide imposant décision |
| BI-10 | Brouillon/session créé par ancienne version, passage direct confirmation | Refus ou recomposition sécurisée des deux services; pas d'ancienne facture mono-service |
| BI-11 | Création puis édition client, ancien `water_enabled=0` | Aucune coercition de raccordement, la facture demeure combinée |
| BI-12 | Migrations `db.initialize()` répétées + base d'avant `water_enabled` | Idempotence, données originales préservées, déclencheurs non coercitifs |

## D. Intégrité/historique/finances (P0)
- DATA-01 : bases issues des versions antérieures (v0.2/v0.3) **copiées/anonymisées** : migrer en environnement de test, assurer `PRAGMA integrity_check=ok`, factures historiques même numéros/montants.
- DATA-02 : poster des paiements et contre-écritures, exporter/importer `sel-facturation-portable` version 1 avec les éléments historiques et nouveaux, aucune double comptabilisation.
- DATA-03 : vérifier triggers d'immutabilité facture/paiement/reversal et protection contre suppression cascade.
- DATA-04 : deux émissions simultanées même client+période, numérotation et contrainte unicité; erreurs de saisie ne consomment pas un numéro.
- DATA-05 : sauvegarde préinstallation, répéter installation/migration; suppression des tarifs référencés doit être refusée si historique l'utilise.
- DATA-06 : PDF historique électrique ne prétend pas enregistrer une eau qui n'existait pas dans son snapshot, et n'est pas modifié lors du recalcul actuel.
- DATA-07 : vérification que le QR interne n'encode ni nom/adresse client, ni vraie ou fausse signature DGI.

## E. Pipeline / Windows / livrables
- `python -m pytest -q` (inclut nouveaux tests ciblés), `python -m compileall -q ...`, `git diff --check`, CI Windows au SHA final, PyInstaller EXE, MSI WiX, wrapper Burn/preflight, vérifications packages.
- Contrôle visuel de 4 PDF anonymes : USD eau 0; USD eau positive; CDF eau 0; CDF eau positive, plus échantillon de facture ancienne; sans débordement A4, somme exacte, code QR lisible en scan indépendant. Pour PDF nouvellement généré, utiliser un rendu PDF/PNG ou extraction pypdf et signaler limites.
- E2E sur Windows compte standard avec installation/upgrade depuis v0.3.29 (et ancien Inno si possible), WebView2 absent, base SQLite préservée, export, impression physique. **Ne pas marquer PASS faute d'accès matériel**.
- SafeUpdate : pas de clé Ed25519 production → déploiement auto-update = NO-GO; test cryptographique de non-contournement et message UX correct. Le paquet unsigned peut déclencher Windows.
- Fournir table `ID | résultat PASS/FAIL/BLOCKED | plateforme | commande | preuve | commit` dans rapport final. Aucun test manuel inventé.

## Conditions de sortie
Aucun P0 sans test vert et rapport. Si CI est rouge après changement, corriger jusqu'au vert; si risque externe bloque, marquer BLOQUÉ et exposer. Une PR correcte mais sans recette PC SEL reste **GO pour revue, NO-GO pour déploiement**.
