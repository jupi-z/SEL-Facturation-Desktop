# Matrice de recette automatisée + manuelle — SEL Final Gate

Principe : d'abord faire échouer les tests ciblés contre le HEAD de départ, ensuite les faire réussir. Utiliser UNIQUEMENT des données synthétiques/DB temporaires. Le statut `PASS` implique preuve réelle; `NOT RUN` ou `BLOCKED` est admis et transparent, jamais converti en PASS.

## A. Montants et conversions (bloquants P0)

| ID | Préparation (fixture synthétique) | Résultat indispensable |
|---|---|---|
| FX-01 | 100 kWh à 0,095 USD, TVA électrique 16 %, 0 m³ sans tarif eau | USD 11,02 ; CDF à 2 360 = **26 007,20** |
| FX-02 | Même cas que FX-01, PDF CDF | EXCLURE **61 376 992,00** ; total, ligne électrique, TVA, solde cohérents |
| FX-03 | 100 kWh, eau 3 m³ à 0,40 USD/m³ et profil eau de TEST sans taxes | USD **12,22** ; CDF à 2 360 = **28 839,20** |
| FX-04 | Deux prestations positives, partial payment 2 USD | Paiement CDF 4 720,00 ; solde USD 10,22 ; solde CDF 24 119,20 |
| FX-05 | Variantes taux 1, 2360, décimal, valeur invalide ou 0 | Contrôle d'arrondi, taux obligatoire >0, aucune double conversion |
| FX-06 | EUR/CDF ou prix tarif différent de la devise facture | Conversion explicite contrôlée OU refus clair avant facture; jamais mauvais label |
| FX-07 | Facture déjà émise puis taux USD/CDF changé | Snapshot et QR d'origine immuables; aucune altération du montant USD; variante CDF indique le taux réellement appliqué |
| FX-08 | Tarif électricité non entier (0,095 USD/kWh), quantité fractionnaire | Arrondi monétaire cohérent, quantité inchangée, prix unitaire imprimé fidèlement sans tronquer ses décimales |

Les taux `0,095/0,40/2360` des fixtures ne constituent pas une validation du tarif de l'eau ou de change réel de SEL.

## B. Eau obligatoire et validation de relevé

| ID | Cas | Résultat |
|---|---|---|
| WA-01 | Ancien abonné avec `water_enabled=0`, volume 0 | Deux services dans preview, snapshot, PDF, CSV/JSON, **0 m³** et montant 0 |
| WA-02 | Nouvel abonné, volume 0, tarif eau absent | Facture validable, 0 visible, prix « — » et aucune fausse TVA |
| WA-03 | Abonné nouveau/ancien, volume 1/3/15 m³, tarif + fiscalité approuvés | Calcul réel par m³, taxes indépendantes, total commun |
| WA-04 | Eau >0, tarif absent/inactif/non confirmé | Bloquer sans créer numéro ni facture |
| WA-05 | Eau >0, fiscalité absente/inactive/non confirmée | Bloquer sans création de facture |
| WA-06 | Eau absente/blanche, -1, NaN, Infinity, > précision max | Refuser; ne pas remplacer par zéro |
| WA-07 | Passage client A↔B, brouillon eau, rechargement/erreur de formulaire | Aucune fuite de valeur d'un abonné à l'autre, confirmation claire pour zéro |
| WA-08 | Même client + période soumis deux fois | Une seule facture active et aucun trou de séquence indu par prévisualisation échouée |
| WA-09 | Les deux consommations 0, sans frais autorisés | Deux lignes + total 0 ; facture valide si règles métier SEL autorisent ce cas |
| WA-10 | Contrat/branchements physiques inconnus | Ligne d'eau 0 sur facture, statut de raccordement non inventé dans fiche client |

## C. Tarifs et fiscalité

- TA-01 : actif 1/confirmé 1/effective date valide => sélectionné pour le bon service.
- TA-02 : actif 0, même période => jamais utilisé, erreur métier.
- TA-03 : tarif plus ancien actif et plus récent inactif => comportement explicite, **jamais reprise silencieuse** non autorisée.
- TA-04 : type résidentiel / commercial / industriel, période passée et future; pas de « fuite » de prix entre types.
- TA-05 : prix du m³ non confirmé, taxe eau non confirmée, change USD/CDF incohérent => pas d'émission positive.
- TA-06 : profils fiscaux actifs/confirmés par service ; `0 %` explicitement confirmé est différent de « inconnu »; aucun calcul fiscal inventé.
- TA-07 : supprimer/archiver un référentiel référencé par une ancienne facture ne change pas son snapshot ni son PDF.
- TA-08 : montant facture = somme des prestations, HT + taxes + frais réels expliqués = total de chaque prestation.

## D. Historique, paiements, export, SQLite

- HI-01 : base de fixture ancienne avec facture électricité seule, après migration -> snapshots et total identiques; PDF fidèle à l'ancienne prestation.
- HI-02 : corrections append-only partielle/complète, annulation, soldes/rapports/PDF cohérents, rejet des corrections excessives.
- HI-03 : CSV/portable JSON v1 reflètent les deux prestations pour factures nouvelles; anciens documents sont toujours exportables.
- HI-04 : installation/upgrade à partir d'ancienne base synthétique avec clients raccordés/non raccordés -> aucune transformation silencieuse des faits métier; migration idempotente.
- HI-05 : absence de client, suppression et archivage, types, devises, paiements, références et sauvegardes restent stables.
- HI-06 : `PRAGMA integrity_check=ok`, contraintes SQLite, transactions concurrentes, sauvegarde et restauration de **copie**.
- HI-07 : un PDF CDF indicatif ne modifie ni SQL ni snapshot ni le JSON d'origine, y compris QR interne.

## E. Génération PDF, UI et compatibilité

- PD-01 : extraction texte PDF vérifie 2 services, `0 m³`, libellés/totaux, non-certification fiscale.
- PD-02 : inspecter pages rendues ou images de PDF (pas seulement extraire texte) : A4, découpage, colonnes, QR, longs champs, zéro tarif eau, taxes multiples.
- PD-03 : titre des factures anciennes ne doit pas ajouter de prestation facturée inexistante.
- PD-04 : prix unitaire et devise de chaque ligne CDF correspondent au montant affiché ; écart d'arrondi explicable.
- PD-05 : HTML preview/détail/PDF/CSV donnent mêmes unités et montants; scénario PDF annulé.
- UI-01 : champ eau non masqué, validation 0 explicite, changement de client et gestion de brouillon.
- UI-02 : erreur serveur n'autorise pas émission d'un ancien aperçu et ne consomme pas de numéro.
- UI-03 : autres écrans (connexion, abonnés, settings, paiements, exports, updates) accessibles sans régression.

## F. Pipeline et sécurité (gates)

- `python -m pytest -q` — suite **entière** et nombre exact de tests; `python -m compileall -q` ; `git diff --check`.
- Workflow Windows sur HEAD de PR : PyInstaller EXE, smoke test démarrage/import, MSI WiX, Burn/Bootstrapper WebView2, tests packaging.
- Contrôles d'authentification updater Ed25519 existants inchangés, pas de fail-open si clés de production absentes.
- Recherche automatisée de secrets/tests fictifs dans les artefacts; ne pas publier d'informations client.
- La réussite de CI n'est **pas** une recette utilisateur : Windows 10 et 11 tests install/upgrade/portable, SmartScreen/UAC, WebView2 offline, PDF impression réelle et reprise backup sur poste non productif.
- En cas de problème d'environnement CI sans lien avec code, le déclarer et expliquer/contourner **par méthode sûre**, pas en désactivant les tests.

## Preuves à joindre à la PR technique

1. Référence des tests ciblés rouge→vert, commandes et sorties.
2. Table calculée des fixtures FX-01 à FX-07 ; captures PDF A4 USD/CDF.
3. Test de migration depuis au moins une DB synthétique représentant ancienne version, preuves d'immuabilité/backup.
4. Commit SHA exact et liens CI verte.
5. Installateur candidat en **artefact CI**, hash et taille mesurés, non publiés comme release.
6. Checklist GO/NO-GO par catégorie : code, données, commercial SEL, fiscal, Windows matériel, SafeUpdate production.
