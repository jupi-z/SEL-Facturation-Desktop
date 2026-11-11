# 03 — Matrice CRUD sûre : Figma ↔ API Rust ↔ actions permises

**Sources contrôlées** : `v2/openapi/sel-v2.yaml`, `v2/crates/sel-api/src/lib.rs` et `v2/windows-app/src-tauri/src/lib.rs`, sur `main@109555533848d441444830748236f74714c2c8fd`. **Vérifier à nouveau le HEAD et les autorisations avant toute implémentation.**

Le mot CRUD désigne des **capacités métier**. `D` n'est pas synonyme de suppression physique : les documents/écritures doivent demeurer audités. La whitelist Tauri peut empêcher une action même quand Rust fournit la route. Ne jamais contourner ses tests de refus de chemins.

| Ressource | Créer | Lire | Modifier/corriger | « Supprimer » logique | Écrans |
|---|---|---|---|---|---|
| Abonnés | `POST /v1/customers` | `GET /v1/customers`, `GET /v1/customers/{id}` | `PATCH /v1/customers/{id}` **avec If-Match** | `status=ARCHIVED`, puis `ACTIVE` (pas de DELETE) | C01–C06, C30–C31 |
| Dessertes eau/électricité | `POST /v1/customers/{id}/service-accounts` | `GET` associé, `GET /v1/service-accounts/{id}` | `PATCH /v1/service-accounts/{id}` **If-Match** (contract_ref, connection_status, activated_at) | Changer statut autorisé, pas de DELETE | O01–O02, C07–C08, C32 |
| Compteurs | `POST /v1/service-accounts/{id}/meters` | `GET` associé | **non exposé** | **non exposé** ; concevoir remplacement/neutralisation non destructive | C09–C10, O03 |
| Relevés | `POST /v1/meters/{id}/readings` | `GET` associé | `POST /v1/meter-readings/{id}/corrections` (append-only, raison, source et index) | Pas de suppression | O04–O05, C11–C12, C33 |
| Factures utilités | `POST /v1/billing/utility/preview`, puis `.../issue` | `GET /v1/invoices`, `/{id}`, `/{id}/document-data` | Pas de PATCH après émission | `POST /v1/invoices/{id}/void` (annulation auditée) | F05–F07, C13–C14 |
| Paiements | `POST /v1/payments` | `GET /v1/payments`, `/{id}/document-data` | `POST /v1/payments/{id}/allocate` | `POST /v1/payments/{id}/reverse` (écriture inverse auditée) | F01–F04, C15–C17, C34 |
| Créances | Créées lors des ventes/factures | `GET /v1/receivables/{id}`, situation abonné | `POST /v1/receivables/{id}/credit` | Avoir auditée ; pas de DELETE | F08, C18–C19, C35 |
| Produits solaires | `POST /v1/admin/catalog/products` (DRAFT) | `GET /v1/catalog/products`, versions admin | Nouveau brouillon / `.../{id}/approve` | `.../{id}/revoke` | A08, C24–C25, C29 |
| Tarifs eau/électricité | `POST /v1/admin/catalog/tariffs` (DRAFT) | `GET /v1/catalog/tariffs`, versions admin | Nouveau brouillon / approve | revoke version | A04–A05, C24–C25, C29 |
| Tax profiles / TVA | `POST /v1/admin/catalog/tax-profiles` | GET catalog/admin | Nouveau brouillon / approve | revoke version | A06, C24–C25, C29 |
| Cours de change | `POST /v1/admin/catalog/fx-rates` | GET catalog/admin | Nouveau brouillon / approve | revoke version | A07, C24–C25, C29 |
| Utilisateurs | `POST /v1/admin/users` | `GET /v1/admin/users` | `PATCH /v1/admin/users/{id}` rôle/statut | Désactiver/révoquer l'accès, sans supprimer utilisateur | A01–A03, C23 |
| Journal d'audit | Création par moteur | `GET /v1/admin/audit-events` | Jamais en UI | Jamais en UI | A09 |
| Ventes solaires | `POST /v1/solar/quotes`, `POST /v1/solar/sales` | `GET /v1/solar/sales`, `/sales/{id}/document-data` | Pas de PATCH confirmé | Pas d'annulation exposée ; évolution API à concevoir | O06–O08, C20–C22 |
| Contrats solaires | `POST /v1/solar/contracts`, `/contracts/{id}/activate` | `GET /v1/installment-plans/{id}` | Avenant non exposé | Résiliation non exposée | O09–O10, C28 |

**Important** : l'API du serveur et la whitelist Tauri doivent être **toutes les deux** vérifiées pour chaque route du tableau. Ne jamais considérer « endpoint existe dans OpenAPI » comme preuve suffisante que l'interface est autorisée ou fonctionnelle.

## Autorisations et risque

- **Source autoritative :** contrôle RBAC effectif du backend, pas une simple condition dans React. L'UI ne montre ou n'active l'action que si la capacité renvoyée par SEL l'autorise ; le serveur rejette toute opération illicite.
- **Relevé /corrections** : OpenAPI précise `ADMIN` ou `SUPERVISOR`.
- **Facture /void** : OpenAPI précise `ADMIN` ou `SUPERVISOR`, et écriture immuable/auditée.
- **Paiement /reverse** : OpenAPI précise `ADMIN`, `CASHIER` ou `SUPERVISOR`.
- **Administration** : routes catalogue et utilisateurs réservées à `ADMIN` selon OpenAPI ; `PATCH users` doit préserver le dernier administrateur actif, révoquer les sessions/grants concernés et auditer la modification.
- **Statuts de compte** : OpenAPI affiche `ACTIVE` et `REVOKED`. Vérifier et corriger tout décalage avec le client `DISABLED`. Ne pas changer arbitrairement les valeurs du domaine.
- Vérifier les règles exactes des avoirs, de l'archivage de l'abonné, des affectations et de l'activation solaire dans les handlers Rust/tests ; elles ne doivent pas être déduites d'une phrase du prototype.

## Contrats de validation

### Concurrence sur abonné ou service

1. GET objet + révision / ETag.
2. Modifier dans un formulaire à état contrôlé.
3. Avant PATCH, afficher le récapitulatif ; fournir `If-Match` serveur.
4. Si `409`, conserver les champs en mémoire, recharger la nouvelle version, afficher les divergences. **Ne pas écraser automatiquement.**
5. Si succès, recharger objet et contexte, puis afficher la version confirmée.

### Paiement

1. Vérifier le client, devise, montant, références et éventuelles créances.
2. Enregistrer via `POST /v1/payments` avec clé d'idempotence adéquate.
3. Allouer via `/payments/{id}/allocate` selon contrat et permissions, éventuellement en plusieurs créances.
4. Pour erreur réelle, contrepasser via `/payments/{id}/reverse` avec montant positif et raison. Ne pas modifier le paiement enregistré ni produire deux écritures inverses à cause d'un double clic.
5. Afficher le reçu depuis données JSON structurées et rendre localement.

### Facture

1. `utility/preview` → montants et taxes de Rust, deux lignes eau/électricité, 0 m³ explicite.
2. Confirmer émission : `utility/issue` avec idempotence, révisions commerciales conformes et états requis.
3. Historique et PDF depuis `document-data`, sans fichier PDF serveur.
4. En cas d'erreur : `invoices/{id}/void` uniquement quand l'API l'autorise ; afficher refus `403/409` sans altérer le document.
5. Préserver toute référence et les snapshots précédents.

## Lacunes à ne pas maquiller

**B1 — Compteur** : absence de PATCH/replacement/désactivation. Proposer ADR + modèles de changement d'appareil, prise en compte d'index et historique ; n'ajouter aucune suppression physique.

**B2 — Vente solaire** : pas de route explicite de rectification / annulation ; requiert décision comptable (vente, créance, paiements, contrat éventuel), transaction auditée et idempotence.

**B3 — Contrat solaire actif** : pas d'avenant, de correction ou résiliation d'échéancier confirmé ; définir garde-fous sur montants déjà payés et recalcul serveur.

**B4 — Brouillons de référentiels** : si l'édition ou abandon d'un DRAFT stocké au serveur est demandé, le contrat actuel ne garantit pas ces mutations. Préférer nouveau brouillon versionné ou proposer API dédiée après décision.

Pour chaque B1–B4, produire un écart `BLOCKED_API` (avec URI Figma, route manquante, scénario attendu et test souhaité). **Ne pas afficher dans l'app un bouton actif prétendant réaliser l'action.**

## Documents et fiscalité

- Aucune API de stockage PDF côté serveur ; données structurées uniquement ; PDF local.
- Facturation eau m³ + électricité kWh sur un même document, toujours deux lignes même si eau = 0.
- TVA 16 % et prix eau initial 1,30 USD/m³ évoqués dans le cadrage mais **configuration contrôlée par SEL**, pas valeur codée en JS sans approbation de version.
- Pas de faux identifiant DGI, QR fiscal ou taxe supplémentaire inventée.
