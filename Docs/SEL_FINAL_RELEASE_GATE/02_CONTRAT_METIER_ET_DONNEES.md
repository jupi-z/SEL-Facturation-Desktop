# Contrat normatif SEL — moteur bi-service, document A4 et sécurité des données

Ces invariants font autorité sur les choix de codage antérieurs lorsqu'ils sont contradictoires. `MUST` = impératif. `MUST NOT` = interdit.

## 1. Contrat de la nouvelle facture

- Une nouvelle facture **MUST** comprendre EXACTEMENT DEUX objets distincts et uniques dans `calc_snapshot.services` : `ELECTRICITY` et `WATER`.
- `ELECTRICITY` unité `KWH`, `WATER` unité `M3`; les deux présents dans l'aperçu, PDF, page détail et exports.
- Une quantité d'eau `0` **MUST** rester présente dans les données et à l'impression, avec un montant de consommation `0.00`, même si le tarif/fiscalité d'eau n'a pas été configuré. Aucun frais fixe d'eau ne doit être inventé.
- Une quantité d'eau `>0` **MUST** échouer en absence de tarif ET de profil fiscal actifs, confirmés et valides à la période facturée. Le message doit permettre à l'agent de résoudre le problème sans interprétation fiscale.
- Un volume absent/blanc **MUST NOT** être assimilé à 0 ; une valeur négative/non-finie/hors précision/ou unité invalide **MUST** être rejetée.
- Les deux montants restent **indépendants**, et `total_invoice = total_electricity + total_water`; aucun croisement d'assiette fiscale. `Decimal` partout. Arrondis explicites au centime, avec politique cohérente de répartition des arrondis.
- Le même client, même période et même statut actif : au plus **une facture** ; numérotation atomique et idempotence sous double-clic/concurrence.
- Ne pas appliquer à l'eau les taux électriques par défaut. Si la TVA pour l'eau n'est pas confirmée, aucune mention « TVA eau 0 % », « exonérée » ou « non imposable » ne doit être affichée comme vérité fiscale.
- Si le service eau est à 0 mais un tarif/prix est connu, le document peut mentionner le prix réel; sans tarif confirmé, afficher « — » pour le prix, 0 pour la quantité et le montant.

## 2. Contrat tarifs / taxes / devises

- Tarif identifié par service, type d'abonné, dates d'effet, devise, unité, prix, statut actif et confirmation. Pour eau positive : source/référence SEL et validation.
- Un tarif désactivé ne peut jamais justifier une nouvelle facture. Si une ligne inactive plus récente remplace un tarif antérieur, **ne jamais réactiver silencieusement l'ancien**; revoir le catalogue avec une règle datée et documenter l'éligibilité.
- Un profil fiscal désactivé, absent ou non confirmé ne peut pas justifier eau positive. Les taux sont bornés et l'assiette est adaptée au service.
- Tarif `M3` en USD et facturation USD : montant = volume × prix, règle de TVA eau seulement si approuvée; idem pour KWH. Si une devise diffère : conversion explicite approuvée **avec taux et date enregistrés** ou refus; aucune supposition qu'un tarif CDF est USD.
- PDF CDF indicatif d'une facture USD : multiplier **exactement une fois** chaque montant pertinent, y compris prix unitaires si affichés en CDF, en gardant quantité inchangée. Aucune donnée persistée/QR interne de référence ne doit être altérée.
- Les taux passés sont liés à l'émission ou explicitement identifiés comme indicatifs dans le PDF de représentation; pas de modification du total original à cause d'un nouveau taux.
- Distinguer montant facturé du paiement : paiements et corrections gardent leur devise et traçabilité.

## 3. Relation abonné — services

- La **présence** des deux lignes est une règle du **modèle de facture**, non une affirmation de raccordement du client.
- Conserver les observations de raccordement réelles si disponibles; ne pas mettre `water_enabled=1` automatiquement comme un « fait terrain » pour les anciens abonnés.
- Une valeur inconnue est **UNKNOWN**, pas « connecté ». Pour les anciens dossiers dont la donnée avait été écrasée, ne pas supposer l'état originel; rapporter l'incertitude.
- Les formulaires ne doivent pas masquer WATER pour un abonné quelconque; afficher le champ eau pour tous.
- Éviter les mises à jour de masse de `clients` sans preuve, les triggers imposant une réalité physique, et les migrations supprimant l'information d'origine.

## 4. Immortalité et auditabilité des documents

- Aucune réécriture de `client_snapshot`, `company_snapshot`, `calc_snapshot` des factures précédentes; numéros, dates, soldes et paiements immuables sauf mécanismes documentés de correction/annulation.
- Paiement : append-only; contre-écriture motivée et datée; ne pas permettre de correction négative, au-delà du restant imputé ou sur facture annulée; les rapports et PDF utilisent le **net**.
- Les anciennes factures électriques seules doivent rester lisibles et ne pas être appelées « facture d'eau » si l'eau ne figure pas dans le snapshot réel.
- La migration SQLite doit être additive/idempotente, testée en copie d'une DB **ancienne**, compatible Windows et sans recul de version non contrôlé; intégrité/contraintes/checkpoint WAL à valider.
- Conserver format `sel-facturation-portable` **v1** compatible, sauf si évolution de schéma versionnée explicitement convenue; inclure séparément les deux services et les contre-écritures sans multiplier les paiements.
- `%LOCALAPPDATA%\SEL Facturation\sel.sqlite3` reste hors dossier d'installation; une mise à jour ne la supprime ni ne la remplace.

## 5. Facture A4, QR, mentions

- Facture nouvelle : titre **Facture d'eau et d'électricité** (ou formulation approuvée par SEL), deux sections séparées, quantité/unité/prix/montant, taxes applicables par service, total et règlement communs, données administratives, dates et références.
- Eau à 0 : `0 m³` lisible dans la section consommation ET la ventilation financière, montant `0,00`; aucune ligne masquée ou pseudo-exonération.
- QR documentaire `SEL_INTERNAL_REFERENCE` : références de document non sensibles (émetteur, numéro, date, devise source, total source). **Jamais** présentée comme QR DGI, DEF, facture certifiée ou preuve fiscale; ne pas encoder client/NIF/coordonnées.
- Une facture annulée porte le statut « ANNULÉ » clairement au PDF; les corrections de paiement n'effacent pas la transaction originale.
- Avant livraison, PDF A4 doit être examiné **visuellement** dans les deux devises et pour (0/0), (100/0), (100/>0), avec longueur de noms élevés, mentions longues, pagination et impression.

## 6. Hors périmètre et arbitrages externes

- Pas d'invention tarif/m³ réel, taxe/exonération/règle de TVA eau, catégorie commerciale SEL, ni homologation DGI.
- Pas de refonte vers ERP cloud, pas de fonctionnalité de paiement électronique non demandée, pas de modification du système de mise à jour en vue de contourner la sécurité Windows.
- La recette matérielle sur PC SEL et validation des tarifs/taxes doivent rester **bloquants pour un GO métier**, même si la CI est verte.
