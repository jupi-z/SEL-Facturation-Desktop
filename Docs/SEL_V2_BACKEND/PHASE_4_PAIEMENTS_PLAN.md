# Phase 4 — paiements, créances et reçus

## Objectif

Rendre les encaissements consultables et utilisables de bout en bout par les API centrale PostgreSQL et locale SQLite, avec sélection des créances, paiements partiels, affectations, soldes, données JSON de reçu et corrections auditables. Rust reste l’unique autorité financière ; aucun fichier PDF n’est créé ou stocké par le serveur.

## État initial vérifié

- Les écritures locales et centrales de paiement, les affectations postérieures, les renversements append-only et les notes de crédit existent.
- Le relevé financier client expose les créances, les factures et les paiements ; l’API locale expose aussi une liste de paiements.
- L’API centrale ne déclare pas de lecture de collection des paiements. Aucune route de données de reçu n’est déclarée.
- L’API ne propose pas de collection paginée des créances ; la sélection se fait actuellement depuis le relevé client.
- Les règles d’idempotence, d’allocation et de concurrence déjà en place seront préservées et renforcées par des tests de parcours HTTP.

## Approche retenue

Compléter les contrats de lecture sans remplacer le moteur de paiements ni ajouter un second ledger. Ajouter des collections bornées et filtrables, puis un contrat `payment document-data` JSON pour que le client puisse produire un reçu localement. Le reçu présente les faits immuables d’encaissement et distingue les corrections ultérieures ; il n’est ni une facture fiscale ni une réponse PDF. Les changements de calcul financier exigent un test rouge préalable et l’emploi de `Decimal`.

## Tâches et critères

1. **Journal des paiements** — collection centrale et locale filtrable par client/statut, limite bornée, ordre stable, portée organisationnelle. Tests HTTP pour résultats, filtres, limites et isolation.
2. **Sélection et suivi des créances** — conserver le relevé client comme source autoritative de sélection dans les deux API ; vérifier qu’il expose le solde net, crédit, statut, devise et échéance, et que l’encaissement sélectionne les créances ouvertes du client. Ne pas introduire un second endpoint/projection redondant.
3. **Données de reçu** — route locale et centrale par identifiant de paiement, enveloppe JSON versionnée, identité client, montant reçu/devise/mode/date, allocations et contre-écritures disponibles, état de synchronisation local. Référence interne stable dérivée de l’identifiant, sans inventer de numérotation commerciale.
4. **Corrections et permissions** — conserver les routes append-only d’annulation/avoir/renversement et vérifier en tests les droits et rejets de motifs/montants invalides, sans suppression ni mutation rétroactive.
5. **Contrat et livraison** — OpenAPI, tests de contrat, tests PostgreSQL réel/SQLite, rapport d’implémentation, CI GitHub sur le SHA final et PR #58 actualisée.

## Hors périmètre de cette phase

- Écrans du nouveau client : Phase 7.
- Mise en page PDF, impression et stockage local du reçu : Phase 8.
- Règles de remboursement externe ou compensation inter-créances : à confirmer par SEL ; aucune règle n’est inventée.
- Recette sur poste, caisse ou imprimante SEL : Phase 10.

## Validation de sortie

- Paiement partiel laisse le bon solde et ne crée aucun échéancier ordinaire.
- Paiement, affectations et corrections sont idempotents, persistants et sans double comptage.
- Les deux API exposent des collections cohérentes et les données nécessaires au reçu.
- PostgreSQL et SQLite démontrent les parcours par tests réels ; OpenAPI correspond aux routes et aux autorisations.
- Aucun PDF/fichier de facture ou de reçu n’est stocké côté central.
- Le dernier commit est poussé sur `monarque/sel-v2-backend`, CI vérifiée, PR #58 ouverte, sans merge dans `main`.
