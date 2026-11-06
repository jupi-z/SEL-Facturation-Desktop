# ADR-001 — Remplacement d’un compteur sans perte d’historique

- **État :** accepté pour implémentation UX3
- **Date :** 6 novembre 2026
- **Périmètre :** compteurs physiques rattachés à un compte de service Eau ou Électricité

## Décision

Un compteur ne peut pas être supprimé ni réécrit pour représenter un nouvel appareil. Son remplacement est une opération atomique : l’ancien enregistrement devient inactif et un nouvel enregistrement est créé pour le même compte de service et la même unité. L’ancien numéro de série demeure réservé par la contrainte existante `meter_serial_per_service_account`.

Les relevés et références de factures restent rattachés à l’identifiant de l’ancien compteur. Aucun index n’est transféré au nouvel appareil : l’agent renseigne le nouvel index de départ lors du prochain relevé. La raison du remplacement est obligatoire et conservée dans l’événement d’audit.

La route `POST /v1/meters/{id}/replace` vérifie l’organisation, verrouille la ligne de l’ancien compteur, refuse un compteur déjà inactif, crée le nouvel appareil, publie les deux changements dans `central_changes` et écrit l’audit dans la même transaction PostgreSQL. Le nouvel appareil commence à la révision 1; l’ancien passe de sa révision initiale 1 à 2.

## Garde-fous

- Le remplacement répété de l’ancien identifiant retourne HTTP 409 et ne crée aucun doublon.
- Le numéro de série reste unique pour le compte de service; la contrainte de base arbitre aussi les requêtes concurrentes.
- La route est réservée à ADMIN, SUPERVISOR et BILLING_AGENT; CASHIER n’y a pas accès.
- Aucun changement n’est apporté aux relevés historiques, factures, paiements ou données de production.

## Validation attendue

Le test `postgres_meter_readings` parcourt la route HTTP contre PostgreSQL, crée un relevé et sa correction, remplace le compteur, vérifie le nouvel appareil et l’historique intact, puis vérifie le refus d’une répétition. Il est compilable localement; son exécution requiert une base de test dédiée et `TEST_DATABASE_URL`.

## Écarts API restant ouverts

Cette décision ne tranche pas les règles comptables des rectifications de ventes solaires (B2), avenants/résiliations de contrats (B3), ni l’édition/abandon des brouillons de référentiels (B4). Aucune action active correspondante ne doit être affichée tant que le contrat API et ses règles métier ne sont pas établis.
