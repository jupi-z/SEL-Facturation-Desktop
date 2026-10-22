# SEL — Matrice de tests et recette : facture combinée eau + électricité

**Base de référence :** `main@baf7a88aae9696b29dff4fd45919daad861c9d7a` (à rafraîchir au lancement de Codex).  
**Principe :** d'abord tests de non-régression, puis tests du nouveau format, puis essai Windows et validation SEL. Aucun test automatisé ne remplace une impression physique.

## A. Commandes de contrôle (adapter à l'environnement)

```powershell
py -3.12 -m venv .venv
.venv\Scripts\python.exe -m pip install -r requirements-build.txt pytest pypdf
.venv\Scripts\python.exe -m pytest -q
.venv\Scripts\python.exe -m compileall -q .
```

Consulter les workflows réels `.github/workflows/windows.yml` et le fichier de version courant ; ne pas inventer de commande de publication. Testez PyInstaller et l'installateur avec le pipeline Windows **sur une branche/PR dédiée**, sans publier de release par défaut.

## B. Matrice fonctionnelle et technique

| ID | Cas | Résultat attendu | Priorité |
|---|---|---|---|
| F01 | Facture électrique ancienne, snapshot `FLAT_KWH` | PDF identique sur le fond, montants et numérotation inchangés | P0 |
| F02 | Facture ancienne à tranches (`METER`) | Historique lisible, aucun recalcul, aucune valeur eau inventée | P0 |
| F03 | Nouvelle facture combinée données validées | Une seule référence, 2 services, 2 sous-totaux et total unique | P0 |
| F04 | Eau tarif/règle absents | Émission combinée bloquée avec erreur métier lisible ; pas d'eau fictive | P0 |
| F05 | Eau consommée en m³ | Unité `m³`, volume et taux liés à l'eau seulement | P0 |
| F06 | Eau facturée au forfait (si retenu par SEL) | Quantité forfaits et prix, aucun index/m³ inventé | P1 |
| F07 | Electricité consommation directe | kWh et prix/kWh ; aucun faux index/numéro compteur | P0 |
| F08 | Electricité avec index réels | 2 index + coefficient + kWh réconciliés | P1 |
| F09 | Catégorie client unique | Pas de duplication du type d'abonné sur 2 panneaux | P1 |
| F10 | « Tarif unitaire HT » isolé | Ligne retirée du récapitulatif ; prix unitaire dans les détails | P1 |
| F11 | Type de service absent/inactif | Ni montant ni section de service artificiels | P0 |
| F12 | Plusieurs devises dans les données d'entrée | Refus de totaliser USD+CDF sans conversion documentée et validée | P0 |
| F13 | Variation tarif après émission | Facture ancienne conserve les chiffres et libellés d'origine | P0 |
| F14 | Paiement partiel/après émission | Solde modifié ; total original et QR interne **inchangés** | P0 |
| F15 | Annulation | Mention « ANNULÉ », même référence, pas de réattribution | P0 |
| F16 | Deux confirmations ou doublon période | Une seule facture active abonné/période | P0 |
| F17 | Texte long/accents/caractères spéciaux | Rendu sans débordement, insertion HTML/PDF neutralisée | P0 |
| F18 | Lignes détaillées nombreuses | PDF multipage, titres et tableaux non tronqués, QR sur la page prévue | P1 |
| F19 | Tarifs de services différents | Taxes et bases séparées, calcul `Decimal` exact | P0 |
| F20 | Taxe eau inconnue | Refus combiné réel ; jamais 16 % par défaut sur eau | P0 |
| F21 | Téléchargement et impression | PDF vectoriel A4, lecteur/imprimante Windows fonctionnels | P0 |
| F22 | NIF/point de service manquant | Aucun faux identifiant/« 05 » ; validation ou absence contrôlée | P0 |
| F23 | Export portable | Enveloppe JSON v1 et données anciennes conservées ; nouvelles clés documentées | P0 |
| F24 | Migration SQLite | Initialisation sur ancienne base sans altérer snapshot/paiement ni numéro | P0 |
| F25 | Paiement d'une facture eau+électricité | Un seul compte client/document avec paiement et solde globaux | P0 |

### Cas de calcul de référence (électricité)

- Photo SEL : **391,64 kWh × 0,095 USD/kWh = 37,2058 USD**, énergie facturée **37,21 USD** ; TVA **16 %** sur 37,21 = **5,95 USD** après arrondi ; **total 43,16 USD**. Vérifier `Decimal` et les bases, sans prendre la photo pour un justificatif des tarifs de l'eau.
- Électricité synthétique : 100 kWh × 0,095 USD = 9,50 USD ; TVA 16 % = 1,52 USD ; total service = **11,02 USD**.
- Eau **strictement synthétique pour tests uniquement** : 10 m³ × **0,40 USD/m³** = 4,00 USD ; régime fictif sans taxe **pour ce test** ; total combiné = 11,02 + 4,00 = **15,02 USD**. Interdire la réutilisation du tarif 0,40 comme configuration SEL.
- Avec frais ou taxes supplémentaires, vérifier l'ordre des arrondis et l'égalité total = somme détaillée affichée, y compris variantes multi-devises.

## C. Tests QR : code interne vs code fiscal

| ID | Vérification | Résultat attendu |
|---|---|---|
| Q01 | Scan avec un téléphone standard | JSON UTF-8 décodable, clés `kind`, `v`, `issuer`, `number`, `issued`, `currency`, `total` |
| Q02 | Type | `SEL_INTERNAL_REFERENCE`, version 1, jamais « DGI » ou « DEF » |
| Q03 | Numéro et données | Correspondent exactement au document enregistré en base, pas au brouillon |
| Q04 | Confidentialité | Aucun client, NIF, adresse, numéro de compteur ou secret dans le QR |
| Q05 | Stabilité | Paiement/annulation/réimpression ne changent pas le contenu du QR ; le PDF porte toujours l'état actuel de la facture si prévu |
| Q06 | Faux fiscal | Aucune mention « certifié DGI », « authentifié DGI », QR fiscal inventé, code DEF arbitraire |
| Q07 | Lisibilité | Zone blanche, dimensions adaptées, contraste noir/blanc ; scanner réel sur papier A4 |
| Q08 | Offline | Génération fonctionne sans Internet ni fournisseur externe |

> Éviter d'ajouter des hash/jetons prétendument « sécurisés » si aucune procédure indépendante de vérification n'existe. Un QR lisible n'est pas une preuve d'authenticité.

## D. Tests visuels A4 indispensables

Produire au minimum 5 rendus PDF et captures PNG de revue visuelle (ne PAS utiliser la photo client comme fausse base de données) :

1. Ancien décompte électrique 391,64 kWh / 43,16 USD ; mise en page rétrocompatible.
2. Document eau + électricité synthétique simple ; sections complètes, total lisible.
3. Eau avec taxes distinctes **configurées pour le scénario de test** ; breakdown exact.
4. Client et adresse très longs, plusieurs lignes de détail, rendu 2 pages ; pas de chevauchement.
5. Facture annulée ou facture avec paiement partiel ; logo, montant restant et QR lisibles.

Inspecter les captures/PNG et non seulement le texte extrait. Vérifier : marges A4, logo non déformé, tableau prix/unité, montants alignés, 2 sections identifiables, footer, pas de titre ou badge de certification fictive, lisibilité N&B, page 2 cohérente, zone QR non amputée par l'imprimante.

## E. Contrôles liés au véritable poste SEL

Faire constater, et **documenter comme NON FAIT** jusqu'à preuve contraire : installation du logiciel sur le PC concerné, sauvegarde avant mise à jour, ouverture dans la fenêtre native, test sur imprimante réelle, scan du QR à partir du papier, lecture du PDF, reprise après redémarrage, restauration sur **copie** d'une sauvegarde, intégrité `PRAGMA integrity_check`, vérification des montants par un représentant SEL.

## F. Critères de GO / NO-GO

**GO pilote interne** : CI verte, tests F01-F25 / Q01-Q08 pertinents, rendu PDF contrôlé, compatibilité des anciens documents prouvée, aucune valeur eau fictive en production et fonctionnement local valide.

**GO exploitation SEL** : en plus, mode d'eau et tarifs/taxes validés par SEL, recette physique Windows/impression/QR et sauvegarde réussies, validation du document par SEL, situation fiscale traitée séparément.

**NO-GO** si : facture combinée émise sans base eau confirmée, total incohérent, antériorité modifiée, QR déguisé en DGI, double facture non contrôlée, export corrompu, installation ou impression non testées.

## G. Rapport final attendu

Dans la PR / un rapport de recette : commit de base, fichiers modifiés et raison, avant/après PDF, liste de tests et comptes exacts, éventuels cas non testés, décisions SEL manquantes, impact migration/sauvegarde, résultat de build, preuve d'essai matériel ou mention explicite de son absence, verdict GO/NO-GO justifié.