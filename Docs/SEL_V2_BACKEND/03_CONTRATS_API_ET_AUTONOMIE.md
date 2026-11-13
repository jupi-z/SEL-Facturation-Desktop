# Contrat API v1 — service central autonome et agent local

## Principes
- **REST JSON, OpenAPI 3.1, DTO versionnés**, errors `application/problem+json` avec code stable; les chemins sont proposés comme **contrat initial** à formaliser dans `v2/openapi/sel-v2.yaml`, puis à tester depuis **CLI/HTTP sans UI**.
- Auth serveur (jetons courts + rotation et RBAC) pour utilisateurs et appareils, pas un mot de passe partagé au client ni un jeton root dans l'application. Local agent ne reconnaît que la session locale du poste avec origine/CSRF protections.
- `Idempotency-Key` exigé sur chaque écriture financière et commande sync; réutiliser une clé avec payload différent → 409; réponse exacte du premier effet renvoyée sur retry.
- GET paginés + filtre auth/organization_id; If-Match/version pour changement concurrent. Les données sensibles ne figurent jamais dans URLs ou logs.
- Les opérations reportables hors ligne utilisent **la même commande/version de domaine** en local, mais une confirmation serveur peut refuser/confliter.

## Endpoints obligatoires P0 (le préfixe `/v1` est inclus)
| Domaine | Routes proposées | Règles |
|---|---|---|
| Contrôle API | `GET /health/live`, `GET /health/ready`, `GET /v1/meta/capabilities` | Sans frontend, version/protocol, aucun secret |
| Auth/admin | `POST /v1/auth/login`, `POST /v1/auth/refresh`, `GET /v1/admin/audit-events`, `POST /v1/devices/enroll`, `POST /v1/offline-grants` | RBAC, événements redacted limités à l’organisation (100 derniers), enrôlement supervisé, scopes |
| Abonnés | `GET/POST /v1/customers`, `GET/PATCH /v1/customers/{id}` | ID UUID, suppression sécurisée, ETag |
| Référentiels | `GET /v1/catalog/products`, `GET /v1/catalog/tariffs`, `GET /v1/catalog/tax-profiles`, `GET /v1/catalog/fx-rates` | Versions validées/datées, lecture cache offline |
| Énergie/eau | `POST /v1/billing/utility/preview`, `POST /v1/billing/utility/issue` | Exactement deux services, 0 M3, snapshot immuable |
| Solaires | `POST /v1/solar/quotes`, `POST /v1/solar/sales`, `POST /v1/solar/contracts`, `POST /v1/solar/contracts/{id}/activate` | Une créance par vente; échéancier figé avec total exact |
| Créances | `GET /v1/receivables/{id}`, `GET /v1/customers/{id}/statement`, `GET /v1/installment-plans/{id}` | Solde calculé sur paiements nets, échéances ≠ dette dupliquée |
| Paiements | `POST /v1/payments`, `POST /v1/payments/{id}/allocate`, `POST /v1/payments/{id}/reverse` | Append-only, références stables, validation concurrente |
| Factures | `GET /v1/invoices/{id}`, `GET /v1/invoices/{id}/document-data` | JSON **uniquement**, facture historique intacte, pas d'endpoint PDF |
| Synchronisation | `POST /v1/sync/push`, `GET /v1/sync/pull?cursor=...&limit=...`, `GET /v1/sync/status` | Enveloppes signées/authentifiées, ack, pagination, conflits |
| LiveUpdate | `GET /v1/config/versions`, `GET /v1/client-resources/manifest` | Métadonnées/ressources génériques signées, jamais PDF facture |

**Local API `http://127.0.0.1:<port>/local/v1/`** : endpoints homologues pour tests et client, avec `/local/v1/sync/status`, `/local/v1/sync/issues`, `/local/v1/sync/trigger` et `/local/v1/updates/status`. `sync/issues` retourne au plus 100 conflits/refus sans le payload des commandes ; les opérations financières conflictuelles restent conservées et ne sont jamais résolues automatiquement. L'agent traduit les commandes offline en outbox, sans proxy aveugle de POST distants. Le local est testable sans UI via HTTP/CLI.

## Exemples contractuels (données synthétiques)
```json
POST /v1/billing/utility/preview
{
 "customer_id":"<UUID_TEST>","billing_period":"2026-11",
 "electricity":{"quantity":"100.000","unit":"KWH"},
 "water":{"quantity":"0","unit":"M3","zero_confirmed":true},
 "currency":"USD","tariff_revision":"<APPROVED_VERSION>"
}
```
La réponse doit détailler exactement deux services `ELECTRICITY`, `WATER`, montants décimaux en chaînes et les règles fiscales utilisées. Une écriture finale référence la prévisualisation et son hash/version; le backend recalcule avant validation pour éviter TOCTOU.

```json
POST /v1/solar/contracts/{id}/activate
{
 "sale_id":"<UUID_TEST>","receivable_id":"<UUID_TEST>",
 "schedule":[{"sequence":1,"due_date":"2026-12-01","amount":"50.00"},
             {"sequence":2,"due_date":"2027-01-01","amount":"50.00"}],
 "currency":"USD","approved_terms_revision":"<VERSION_TEST>"
}
```
Cet exemple **n'invente aucun prix SEL**; c'est une fixture de contrôle. L'API vérifie que somme échéances = principal et que la créance du contrat est unique.

```json
POST /v1/payments
{
 "customer_id":"<UUID_TEST>","received_amount":"15.00",
 "currency":"USD","method":"CASH",
 "allocations":[{"receivable_id":"<UUID_TEST>","amount":"15.00"}],
 "occurred_at":"<UTC_ISO8601>"
}
```
Le paiement partiel laisse un solde, **sans** créer de plan d'échéances si l'obligation est une facture eau/électricité ordinaire.

## Erreurs et statuts
- `201` création serveur; `200` idempotent retry; `202` local « pending sync » **pas accepté central**; `409` conflit de révision / même période / règle métier; `422` tarif/taxe incomplet, monnaie incompatible, quantité invalide; `401/403` auth/autorisation; `503` réseau distant (local reste utilisable selon grant).
- Définir `status`: `LOCAL_PENDING`, `CENTRAL_ACCEPTED`, `CONFLICT_REQUIRES_REVIEW`, `REJECTED`, avec transitions autorisées. **Ne jamais répondre « facture définitive synchronisée » en 202 local**.
- `document-data` renvoie des données immuables, `document_schema_version`, `template_version`, `document_reference`, montants, taxe par service, référence QR interne, paiement et solde **au moment demandé** (la facture est immuable, la situation des paiements est une projection séparée), `origin` / statut sync; aucun binaire.
- Pour les paiements, `PAYMENT_RECEIPT` passe en `document_schema_version: 2` : le commit PostgreSQL du paiement conserve un snapshot JSON des références lisibles, du principal, des règlements/avoirs antérieurs, de l’affectation, du solde après versement et des fonds non affectés. Les versions 1 historiques restent lisibles, sans reconstitution inventée de leur solde au jour du paiement.

## Preuves
- OpenAPI validée par un parseur, schémas des erreurs, commandes exemples et tests contractuels.

## Contrat d'autorisation des routes (révision du 3 novembre 2026)

Les routes protégées sous `/v1` sont autorisées par une matrice explicite méthode + chemin + rôle ; toute combinaison non classée est refusée. Les segments `{id}` doivent être des UUID valides. Une nouvelle route ne reçoit donc aucun droit par héritage.

| Opération | Rôles autorisés |
|---|---|
| Lecture des routes explicitement listées dans OpenAPI (dont relevés clients, documents structurés et statut de synchronisation) | ADMIN, BILLING_AGENT, CASHIER, SUPERVISOR, AUDITOR_READONLY |
| Création/modification clients, aperçu/émission de facture, vente/devis/contrat solaire | ADMIN, BILLING_AGENT, SUPERVISOR |
| Encaissement, affectation et contre-écriture de paiement | ADMIN, CASHIER, SUPERVISOR |
| Synchronisation push | ADMIN, BILLING_AGENT, CASHIER, SUPERVISOR ; chaque enveloppe est ensuite contrôlée pour le rôle métier de l'acteur, l'acteur actif, l'appareil actif, la signature du grant, son périmètre, son expiration et ses limites |
| Annulation de facture, avoir/crédit, enrôlement/révocation d'appareil, émission de grant | ADMIN, SUPERVISOR |
| Provisionnement/rotation de mot de passe d'un utilisateur | ADMIN uniquement |

Les sessions sont individuelles. La rotation de mot de passe vérifie à nouveau le rôle ADMIN dans la transaction SQL, modifie le hash Argon2id, révoque les sessions actives de la cible et écrit un audit expurgé dans la même transaction. Aucun mot de passe ni hash n'est écrit dans l'audit.

Le démarrage commercial exige `DATABASE_URL`, un UUID d'organisation non nul, un nom d'organisation non vide, le mode de sessions individuelles et la clé publique valide pour les grants signés. Le service local exige les UUID non nuls et persistés d'organisation et d'appareil ; aucun UUID nul ne constitue une identité valide.
- Démarrage central + PG par `docker compose -f v2/deploy/compose.test.yml up`; un appel CLI doit pouvoir créer client, facture, plan solaire, payer en partie, lire solde et afficher les données JSON pour rendu.
- Test serveur : recherche automatique et assertion aucun `Content-Type: application/pdf` ni voie de persistance de fichier facture; tests de contrôle d'accès à un customer_id non autorisé.
