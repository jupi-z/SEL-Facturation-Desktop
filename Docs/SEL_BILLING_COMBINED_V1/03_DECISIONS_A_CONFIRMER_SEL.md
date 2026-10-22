# SEL — Décisions métier restant à confirmer

**Attention :** ces éléments NE SONT PAS des paramètres validés par défaut. Codex doit bloquer les comportements non déterminés plutôt que compléter arbitrairement.

## Informations réellement manquantes

| Décision | Ce qui est déjà décidé | Ce qui doit être confirmé par SEL | Solution sûre en attendant |
|---|---|---|---|
| Périmètre du document | Une facture unique eau + électricité | Peut-on aussi émettre exceptionnellement un service seul ? | Nouveau mode combiné strict ; conserver ancien mode électrique uniquement pour rétrocompatibilité |
| Eau : unité de facturation | Service eau distinct | m³ au compteur, consommation déclarée, forfait ou autre | Aucun montant d'eau réel ni mode choisi par défaut |
| Eau : tarif et assiette | Prix indépendant de l'électricité | Prix, catégories, date d'effet, devise, frais et traitement fiscal applicables | Eau non émettable tant que non configurée et validée |
| Eau : index/compteur | Infos séparées des kWh | Numéro de compteur eau, index et coefficient exigés ? | Afficher seulement les valeurs renseignées ; ne pas imiter les index électriques |
| TVA et autres taxes eau | Taxation séparée obligatoire côté modèle | TVA applicable ?, taux ?, exemption ?, montant non taxable ?, taxe locale ? | Aucune TVA « 16 % » copiée automatiquement de l'électricité |
| Électricité | kWh et tarif existants dans le programme | Dernières grilles, classes, éventuelles tranches, taxes et prix validés | Préserver et ne pas recalculer les historiques |
| Type d'abonné rayé | Éviter la duplication | SEL voulait-elle changer la catégorie ou seulement retirer le doublon ? | Ne montrer qu'une occurrence, sans changer la valeur métier |
| Annotation « 05 » | Près du point de service | Référence du point de service, code d'agence, numéro de zone ou autre ? | Ne pas figer « 05 » |
| Titre du document | Eau + électricité ensemble | « Décompte » ou « Facture » dans le circuit de délivrance autorisé ? | « DÉCOMPTE DE CONSOMMATION — EAU & ÉLECTRICITÉ » tant que la procédure fiscale n'est pas établie |
| QR | QR interne distinct du futur QR DGI | SEL souhaite-t-elle ultérieurement une page de consultation/validation ? | QR de référence interne, sans URL ni promesse de vérification |
| Paiements | Total à payer commun | La répartition des encaissements par service est-elle requise ? | Paiement global d'une facture, comme actuellement |
| Contacts et références | Reprendre la configuration SEL | Adresse, contact, RCCM, NIF, ID NAT, période et échéance | Ne jamais inventer ou copier les exemples historiques automatiquement |
| Date de dépôt de la facture | Exigence sectorielle à faire confirmer | Disponibilité/suivi de la date de remise chez SEL | Champ distinct si la procédure est applicable, sans date fictive |

## Validation simple recommandée avec SEL

Faire confirmer par un interlocuteur habilité, de préférence par écrit :

- Le mode de fourniture de l'eau et son unité réelle.
- La grille d'eau exacte et sa date d'effet, avec preuve/document.
- Les taxes à appliquer séparément à l'eau et à l'électricité, leur base et le responsable de validation.
- Le libellé d'en-tête souhaité et le statut fiscal réel des documents émis.
- Le numéro ou l'utilisation du point de service annoté « 05 ».

Le logiciel doit refléter leur réponse, pas la deviner.