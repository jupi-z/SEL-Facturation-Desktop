# Architecture normative et décisions ADR — SEL V2

## ADR-001 — monolithe modulaire Rust et API-first (ACCEPTÉ)
Un seul serveur `sel-api` Axum/Tokio déployable et une bibliothèque `sel-domain` réutilisée par `sel-local` (service Rust local). `sel-api` accepte les commandes JSON; aucun calcul monétaire n'est implémenté en duplication dans l'UI. SQLx avec migrations explicites pour PostgreSQL (serveur) et SQLite (poste local). Générer OpenAPI et assurer un test de cohérence de ce contrat.

Définir `/health/live`, `/health/ready` et `/v1`; le serveur fonctionne **sans lancer un frontend, navigateur, session pywebview ni un générateur de PDF**. Le local écoute `127.0.0.1` seulement, endpoint distinct (port configurable), authentification/token éphémère par processus plus protection origine/CSRF si navigateur; pas de binder à 0.0.0.0 sur poste.

## ADR-002 — offline-first à cohérence éventuelle (ACCEPTÉ)
Le client écrit en SQLite dans une transaction atomique **commande + journal append-only + outbox**. Un worker Rust pousse des enveloppes signées/authentifiées par appareil au serveur; le serveur vérifie autorisations, unicité (idempotence), conflits et applique en transaction PostgreSQL; le client reçoit les accusés et télécharge les changements via un curseur. **At-least-once transport / exactly-once business effect via déduplication**, pas promesse réseau exactly once.

Distinguer une commande validée localement d'une facture **acceptée centralement**. Les données en attente ne sont **pas** annoncées comme déjà synchronisées. Les rapports consolidés incluent seulement le confirmé central, plus un compteur local « en attente ».

## ADR-003 — stockage du serveur : données structurées uniquement (ACCEPTÉ, non négociable)
PostgreSQL garde `invoices`, `invoice_lines`, `tax_snapshots`, `payments`, `receivables`, `document_template_version`, `qr_internal_reference`, numéro, date, base/devise/taux, événements et traces; **aucune colonne PDF bytes**, aucun `bytea` facture, aucune table de pièces jointes factures, aucun stockage S3 de factures, aucun fichier PDF de facture sur disque local serveur, dans volumes Docker, uploads, caches HTTP, logs, backups ou exports serveur.

Le client produit le PDF A4 **sur demande** à partir du `document-data` JSON et du **template versionné disponible localement**. Si le client est offline, il lit un snapshot figé de son SQLite. Des vérifications d'identité du snapshot et du template accompagnent les données; des versions historiques doivent rester rendables. QR interne = référence SEL, aucune fausse fiscalisation DGI. **Le serveur n'expose pas `/pdf`, `/download-invoice-file`, ni une réponse PDF.** Les tests de non-stockage recherchent toute tentative de sortie de fichier de facture.

Le rendu PDF du client existant (ReportLab) peut être **réutilisé uniquement dans l'adaptateur client** de test, jamais déménagé dans `sel-api`. Conserver localement les PDF uniquement si l'opérateur choisit explicitement « Enregistrer », avec politique de confidentialité.

## ADR-004 — créances / paiements (ACCEPTÉ)
Vente/facture crée une obligation financière; une facture partiellement payée laisse simplement un **solde dû**, elle ne crée pas automatiquement un échéancier. Une vente solaire peut avoir un **plan d'échéances** : il répartit l'exigibilité de l'obligation **existante**, sans créer une seconde dette ni doubler le chiffre de ventes. Les paiements sont enregistrés une fois et **affectés à l'obligation** ou à une échéance selon contrat, avec contre-écriture append-only, jamais `UPDATE balance = ...` sans audit.

## ADR-005 — devises et arrondis (ACCEPTÉ)
Montants sur serveur `NUMERIC`, Rust `rust_decimal::Decimal` et SQLite chaînes décimales canoniques (ou entiers d'unité mineure par devise). Jamais `f32/f64`. Chaque facture/obligation possède **une devise de référence**; taux de change éventuellement approuvé et historisé par date/source. Calcul service séparé eau/électricité; règles fiscales séparées. Snapshots de tarifs, quantités, taux et taxes figés à l'émission. Politique d'arrondi explicitée dans l'ADR puis testée avec cas limites.

## ADR-006 — mises à jour (ACCEPTÉ)
`LiveUpdate` : configuration/référentiels, schémas JSON, ressources statiques d'interface (plus tard) et modèles déclaratifs compatibles versionnés, signés Ed25519/hash SHA-256 + anti-rollback + activation atomique et retour arrière. **Jamais de scripts arbitraires exécutés depuis Internet** ni changement de droits natifs par simple bundle dynamique. `SafeUpdate` reste la voie de mise à jour native Rust/Windows (peut nécessiter remplacement exécutable), sans Authenticode pour l'instant mais avec protocole cryptographique interne obligatoire.

## ADR-007 — déploiement (ACCEPTÉ)
Déploiement d'abord **local reproductible** PostgreSQL et API Rust (Docker Compose test avec volume DB, TLS via reverse proxy pour cible), puis environnement de staging isolé, puis production **seulement après autorisation et tests de restauration**. PostgreSQL jamais public. Pas de fichiers factures sur le serveur, même sous une forme « archives pour confort ».

## Références officielles
- Axum : https://docs.rs/axum/latest/axum/
- SQLx/migrations : https://docs.rs/sqlx/latest/sqlx/macro.migrate.html
- SQLite WAL et contraintes : https://www.sqlite.org/wal.html
- OWASP API Top 10 : https://api-security.owasp.org/editions/2023/fr/0x11-t10/
- TUF signed metadata : https://theupdateframework.io/docs/metadata/
