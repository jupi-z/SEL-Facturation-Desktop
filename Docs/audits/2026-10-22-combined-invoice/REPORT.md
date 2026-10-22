# Recette synthétique — facture unique eau + électricité

Date : 22 octobre 2026  
Branche : `monarque/combined-invoice-water-electricity`  
Base : `d5f4eb2`  
Statut : PR à soumettre, non fusionnée, aucune release publiée.

## Périmètre livré

- Snapshot additif `COMBINED_WATER_ELECTRICITY`, version 2, avec deux services séparés, sous-totaux, taxes explicites et total réconcilié.
- Rendu ReportLab vectoriel A4 pour un document combiné, avec titre transitoire « Décompte de consommation — Eau & Électricité ».
- QR local `SEL_INTERNAL_REFERENCE` v1, limité à la référence, date, devise et total initial ; il reste stable après paiement et réimpression.
- Lecture de la fiche, prévisualisation de structure, PDF original, variante CDF et export CSV/JSON sans réécrire les anciennes factures.
- Mode de saisie combiné visible mais émission réelle volontairement bloquée tant que SEL n’a pas confirmé le mode eau, l’unité, le tarif et les taxes.

## Preuves visuelles

Les fichiers `samples/*.pdf` sont entièrement synthétiques. Les valeurs eau utilisées (`0,40 USD/m³`, taxes eau explicites vides) sont des fixtures de test uniquement ; elles ne constituent ni un tarif ni une règle SEL.

| Fichier | Cas vérifié |
|---|---|
| `combined-usd.pdf` | Document combiné USD, deux sections et QR interne |
| `combined-cdf-conversion.pdf` | Variante de présentation CDF avec taux affiché |
| `combined-long-text.pdf` | Textes longs échappés et contenu utilisateur |
| `combined-void.pdf` | Document annulé conservé et marqué |
| `combined-partial-payment.pdf` | Paiement partiel, solde distinct du total QR |

## Résultats automatisés

À compléter avec les résultats finaux de la PR :

- `python -m pytest -q`
- `python -m compileall -q .`
- `git diff --check`
- Vérification A4 via `pypdf` et extraction de texte.
- Vérification du payload QR : clés strictement limitées au contrat `SEL_INTERNAL_REFERENCE v1` ; aucun nom, adresse, NIF, compteur, secret, URL ou mention DGI.

## Compatibilité et limites

- Les snapshots électriques historiques sans `document_kind` restent rendus par le chemin PDF historique.
- Le format portable `sel-facturation-portable` reste en `schema_version: 1`; les clés combinées sont conservées dans le snapshot sans migration destructive.
- Le QR interne n’est pas une authentification, une signature, un reçu de paiement ou un QR fiscal DGI.
- Aucun tarif, taux ou régime eau réel n’a été inventé ; aucune émission combinée réelle ne doit être activée avant confirmation écrite de SEL.
- La recette d’installation, d’impression et de scan sur le poste Windows cible n’a pas été exécutée dans cet environnement : **pilote non validé sur poste cible**.

## Décisions restant à confirmer par SEL

1. Libellé commercial définitif (« Décompte » ou « Facture »).
2. Mode eau : compteur m³, volume direct ou forfait.
3. Unité, tarif, catégories, taxes/exonérations et règle d’arrondi de l’eau.
4. Champs eau à afficher (index, compteur, référence contractuelle) et modalités de remise.
5. Validation visuelle A4 et test du QR avec le matériel cible.
