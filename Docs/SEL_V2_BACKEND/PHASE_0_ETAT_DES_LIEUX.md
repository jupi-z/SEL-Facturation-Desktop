# SEL V2 — Phase 0 : état des lieux et décisions de cadrage

Date : 3 novembre 2026  
Branche : `monarque/sel-v2-backend`  
PR : [#58](https://github.com/jupi-z/SEL-Facturation-Desktop/pull/58)  
État : **IN_PROGRESS — 76 %**

Ce document consigne les preuves de cadrage et les points encore à valider. Il ne certifie ni une recette Windows SEL, ni une installation cliente, ni le choix final du framework desktop.

## Résumé vérifiable

| Sujet | État | Preuve / réserve |
|---|---|---|
| Worktree et branche | PASS au contrôle | Worktree propre avant cette mise à jour documentaire ; branche locale synchronisée avec `origin/monarque/sel-v2-backend`. |
| PR et référence distante | PASS | PR #58 ouverte vers `main`. SHA de code contrôlé : `ee62737ab7177bffb530681cef8d86750abc39df`. |
| CI sur le dernier SHA de code | PASS | 4/4 workflows réussis sur `ee62737` : Rust/PostgreSQL, paquet serveur, Windows Desktop et candidat Windows V2. Un commit documentaire de suivi peut relancer la CI. |
| Documents normatifs | PASS, avec préséance | Les exigences backend restent applicables. Le mandat propriétaire ultérieur du 3 novembre autorise le nouveau frontend V2 et remplace l’interdiction initiale de frontend pour ce périmètre seulement. V1 et ses données restent protégées. |
| Inventaire Figma | PASS pour le comptage | 40 écrans métier + 5 modèles A4 : 30+4 annoncés dans la livraison et 10+1 ajoutés le 3 novembre. Le tableau de bord sombre est une variante du même écran. |
| Cartographie écran/API/tests | PARTIAL | Les 40 écrans sont reliés aux contrats et lacunes API ; les tests backend existants sont reliés aux parcours. Vérifier les droits exacts par handler et construire les tests UI en phase d’implémentation. |
| Paquet déjà remis à SEL | BLOCKED_EXTERNAL | Le dépôt identifie une release publique V1, mais aucun journal d’installation, reçu de livraison ou inventaire du poste SEL ne prouve le paquet actuellement installé. |
| Spike du shell Windows | BLOCKED_EXTERNAL | Environnement de contrôle : Windows 11 Pro x64, Node 24.21.0, npm 11.19.0, .NET SDK 8.0.426 et WebView2 Runtime 156.0.4314.58 présents ; `cargo`/`rustc` et Visual Studio C++ Build Tools non détectés. Aucun build Tauri n’a été prétendu. |

### Référence GitHub et paquet

- HEAD contrôlé : `ee62737ab7177bffb530681cef8d86750abc39df`, branche propre et alignée avec son upstream au moment du contrôle.
- La PR #58 est ouverte, base `main`, dont le SHA distant contrôlé est `09d9a244b6b243fccdf3313d0a9d1aace2daa071`.
- Sur le SHA de code `ee62737`, les quatre workflows sont réussis : Rust `41704810520`, serveur `41704810691`, Windows Desktop `41704810699`, candidat Windows V2 `41704810645`. La construction du candidat CI n’est ni un test d’installation sur poste SEL ni la validation du nouveau frontend Figma.
- Dernière release V1 non brouillon trouvée : `v0.3.29`, préversion GitHub, avec `SEL-Facturation-Setup-0.3.29.exe` (SHA-256 `0db274ae2ed55845b8598b6f6f652f30d8590aac3f0fac10965bf5a4c58d2cda`), EXE autonome et MSI. `v0.3.31` est un brouillon sans assets. La version actuellement installée chez SEL demeure inconnue.
- Le workflow candidat V2 produit encore `SEL-Facturation-V2-Setup-0.3.30.exe` via PyInstaller/pywebview ; ce n’est pas le nouveau frontend Figma et ce nom/version ne prouve pas une publication ou une remise à SEL.

### Inventaire Figma réconcilié

Pages principales : Vision, Fondations, Composants, Parcours, Opérations Windows, Solaire/finance, Administration/système, Documents A4, Spécification de livraison, États/accessibilité.

| Famille | Écrans recensés |
|---|---|
| Opérations et services — 10 | Vue d’ensemble, répertoire abonnés, dossier abonné, nouvelle facture, prévisualisation, encaissement, historique des factures, relevés compteurs, relevé eau au m³, facture avec eau consommée. |
| Solaire et finance — 11 | Catalogue, nouvelle vente, contrat/échéancier, suivi créances, situation abonné, journal de caisse, corrections, rapports, édition catalogue, vente comptant, aperçu de vente comptant. |
| Accès, administration et système — 19 | Connexion, activation appareil, synchronisation, résolution conflit, utilisateurs/rôles, appareils/grants, tarifs/taxes, devises/taux, audit, mises à jour, impression/documents, sauvegardes/diagnostics, identité SEL, mentions éditeur, paramétrage eau/TVA, comptes initiaux/permissions, remise client, révision tarif eau, matrice des droits. |
| Documents A4 — 5 | Facture combinée acceptée, facture combinée provisoire, facture de vente solaire, reçu, facture combinée avec eau à 5 m³. |

Les cadres de conception Windows sont en 1440×900 ; les documents sont A4 portrait. Tokens relevés : Geist, bleu nuit `#012A5E`, bleu d’action `#075BBB`, jaune `#FDDC02` comme accent, fond `#F7F9FC`, séparateurs fins et états communiqués par texte et couleur. Les exemples Figma sont des données fictives. Les coordonnées légales, tarifs, taxes, change et paramètres SEL doivent venir de référentiels approuvés.

Captures observées via le connecteur Figma : dashboard, aperçu facture et facture A4. L’examen de toutes les frames a porté sur leurs noms/dimensions et leur inventaire ; ce n’est pas encore une revue visuelle détaillée de chaque écran.

### Écart initial entre Figma, OpenAPI et routes

| Parcours Figma | Routes présentes observées | Lacune ou vérification requise |
|---|---|---|
| Abonnés et dossier | GET/POST/PATCH clients centraux et locaux, relevé du client | Pas de route d’archivage/suppression ni endpoints central de recherche avancée clairement contractualisés. |
| Relevés eau/électricité | Tables de compteurs/relevés côté PostgreSQL | Pas de routes CRUD de compteurs/relevés identifiées dans les routeurs centraux ou locaux. |
| Facturation et historique | Preview/issue, lecture facture, `document-data`; liste locale | Pas de liste/recherche centrale de factures dans l’OpenAPI/routeur. Le nouveau client doit rendre/ imprimer localement ; ce rendu n’est pas fourni par le client V2 actuel. |
| Paiements, caisse, créances | Créer, allouer, contre-écrire un paiement ; lire créance, échéancier et relevé client | Pas de liste centrale de paiements/journal de caisse ni de liste paginée de créances identifiée. |
| Kits solaires | Quote, sale, create/activate contract, read installment plan ; lecture catalogue produits | Pas de création/modification/désactivation de produit ou catalogue ; confirmer endpoints d’historique et états de stock avant d’afficher une disponibilité. |
| Administration | Login/refresh/logout, provisionnement de mot de passe, enrôlement/révocation appareil, offline grants ; lecture tarifs/taxes/change | Pas de listes/CRUD utilisateurs et rôles, catalogue d’appareils, écriture des tarifs/taxes/devises ou journal d’audit identifiés. |
| Reporting et opérations | Santé, versions de configuration, manifeste ressources | Pas d’API de reporting, de résolution de conflit dédiée, sauvegarde/restauration ou diagnostics d’exploitation identifiée dans l’API UI. Les opérations CLI ne valent pas automatiquement écrans/API. |

**Drift OpenAPI confirmé par comparaison statique des routeurs :** routes existantes mais absentes du fichier `v2/openapi/sel-v2.yaml` : `POST /v1/auth/login`, `/v1/auth/refresh`, `/v1/auth/logout`, `POST /v1/users/{id}/password`, `POST /v1/invoices/{id}/void` et `POST /v1/payments/{id}/reverse`. À synchroniser avant de générer le client typé.

Les écrans ne doivent pas masquer ces absences par des données de démonstration. La matrice de cadrage ci-dessous couvre les 40 écrans recensés ; les noms de rôles sont les rôles attendus du cahier des charges, à confirmer avec les politiques effectivement appliquées par chaque handler.

### Matrice de cadrage des 40 écrans

`LOCAL` désigne l’API `/local/v1/*`; `CENTRAL` désigne `/v1/*`. La matrice indique si une route existe, pas si l’écran est déjà livré. Aucun nouvel écran n’est implémenté.

| # | Écran Figma | Contrat de données attendu | Accès métier attendu | État/API ou validation restant |
|---:|---|---|---|---|
| 1 | Vue d’ensemble | projections clients/factures/paiements/sync | opérateur selon permissions | agrégat dashboard/reporting absent ; isoler local pending des totaux centraux |
| 2 | Répertoire des abonnés | `GET/POST /v1/customers`, équivalents LOCAL | BILLING_AGENT, ADMIN | liste/création présents ; vérifier filtres, pagination et état vide |
| 3 | Dossier abonné | `GET/PATCH /v1/customers/{id}`, `/statement` | BILLING_AGENT, CASHIER selon onglet | fonctions présentes ; journal complet par client et archivage non définis |
| 4 | Nouvelle facture | `POST /billing/utility/preview`, puis `/issue` | BILLING_AGENT | deux routes locales et centrales ; test de parcours bout en bout requis côté nouveau client |
| 5 | Prévisualisation facture | preview + données calculées par Rust | BILLING_AGENT | route de preview existe ; rendu A4 local absent du client V2 actuel |
| 6 | Encaissement | `POST /payments`, `/payments/{id}/allocate`, relevé client | CASHIER, SUPERVISOR | écriture/allocation existent ; sélectionner et recharger créances ouvertes ; reçu local à construire |
| 7 | Historique des factures | liste paginée, filtres, `GET /invoices/{id}` | BILLING_AGENT, AUDITOR_READONLY | lecture par ID et liste LOCAL ; liste/search CENTRAL manque |
| 8 | Relevés de compteurs | CRUD compteur/relevé, historisation index | BILLING_AGENT | table PostgreSQL existe ; routes centrale/locale et validation de période absentes |
| 9 | Relevé d’eau au m³ | API readings WATER, tarif applicable | BILLING_AGENT | aucun endpoint de relevé ; tarifs lisibles, mais administration absente |
| 10 | Facture avec eau consommée | preview/issue combinées | BILLING_AGENT | moteur preview/issue existe ; recette eau>0 exige tarif et profil fiscaux approuvés |
| 11 | Catalogue des kits solaires | `GET /catalog/products` | BILLING_AGENT, ADMIN | lecture active existe ; création/modification/activation et état stock manquent |
| 12 | Nouvelle vente solaire | quote, `POST /solar/sales` | BILLING_AGENT | devis/vente existent ; vérifier prix produit serveur et preview/document |
| 13 | Contrat et échéancier | create/activate contract, GET installment plan | BILLING_AGENT, SUPERVISOR | création/activation/lecture présentes ; édition avant activation et paiements liés à l’échéance à valider |
| 14 | Suivi des créances | liste receivables + échéances/soldes | CASHIER, SUPERVISOR, AUDITOR_READONLY | lecture par ID et plans existe ; listing paginé manque |
| 15 | Compte et situation abonné | `GET /customers/{id}/statement` | BILLING_AGENT, CASHIER, AUDITOR_READONLY | route existe ; confirmer inclusion des pending locaux et séparation central/local |
| 16 | Journal de caisse | liste paiements, affectations, filtres et clôture | CASHIER, SUPERVISOR, AUDITOR_READONLY | liste paiements centrale/journal/agrégats manquent |
| 17 | Corrections financières | reverse payment, credit note, void invoice | SUPERVISOR, ADMIN | handlers présents ; certaines routes manquent OpenAPI ; trace append-only à afficher |
| 18 | Rapports financiers | API reporting avec totaux par devise/statut | SUPERVISOR, ADMIN, AUDITOR_READONLY | endpoint absent ; chiffres de démo interdits en production |
| 19 | Édition du catalogue solaire | create/update/deactivate product | ADMIN | routes d’écriture absentes |
| 20 | Vente solaire comptant | quote + sale, sans plan d’échéances | BILLING_AGENT | route vente existe ; distinguer cash du contrat à crédit sans doubler créance |
| 21 | Prévisualisation solaire comptant | quote/document-data local | BILLING_AGENT | endpoint dédié de preview/document solaire non identifié ; document local requis |
| 22 | Connexion sécurisée | `POST /auth/login`, `/refresh`, `/logout` | tous les utilisateurs actifs | routes présentes au serveur mais absentes OpenAPI ; API locale/session UI à définir |
| 23 | Activation du poste | enroll device + créer grant signé | ADMIN/SUPERVISOR, avec agent activé | enroll/grant présents ; parcours de code d’activation/invitation et liaison organisationnelle incomplets |
| 24 | Centre de synchronisation | push/pull/status central, status/trigger LOCAL | opérateur selon droits, ADMIN | routes présentes ; l’UI neuve devra représenter ACK, en attente, rejet et conflit |
| 25 | Résoudre un conflit | liste de conflits, comparaison, commande de résolution | SUPERVISOR, ADMIN | push renvoie conflits, mais endpoint list/review/resolve dédié non identifié |
| 26 | Utilisateurs et rôles | liste/création/suspension, attribution RBAC | ADMIN | login et provisionnement de mot de passe existent ; CRUD utilisateur/roles manquant |
| 27 | Appareils et grants offline | liste/révocation, émission et statut grant | ADMIN/SUPERVISOR | enroll/revoke/create grant présents ; listing et détail d’appareil/grants manquent |
| 28 | Tarifs et taxes approuvés | lecture catalogue et écritures historisées | ADMIN, approbation SUPERVISOR | GET produits/tarifs/taxes existe ; mutations/versionnement d’approbation absents |
| 29 | Devises et taux de change | `GET /catalog/fx-rates`, mutation historique | ADMIN, approbation SUPERVISOR | lecture existe ; mutation et workflow d’approbation absents |
| 30 | Journal d’audit | recherche événements append-only | AUDITOR_READONLY, ADMIN | stockage d’événements existe ; endpoint de consultation filtrée absent |
| 31 | Mises à jour et intégrité | versions/config manifest + statut local updates | ADMIN, opérateur en lecture | routes de manifest/status existent ; le canal production et les clés restent à valider séparément |
| 32 | Impression et documents | `GET /invoices/{id}/document-data`, templates locaux | BILLING_AGENT, CASHIER | JSON structuré existe ; PDF, aperçu, sauvegarde/impression et reçu locaux à construire |
| 33 | Sauvegardes et diagnostics | opérations locales/serveur restreintes | ADMIN | CLI backup/restore existe selon le rapport ; API UI/états de diagnostic manquants ; restore non productif à prouver |
| 34 | Identité officielle SEL | lecture/édition profil organisation | ADMIN | `organizations.legal_name` existe ; route et champs juridiques complets manquent |
| 35 | À propos/éditeur | ressources locales statiques | tout utilisateur | pas de besoin API ; distinguer clairement SEL du développeur LTEX/MonarQue |
| 36 | Paramétrage tarif eau et TVA | catalogue/règles avec version/approbation | ADMIN, approbation SUPERVISOR | lecture seule aujourd’hui ; toute taxe ou tarif non approuvé doit rester bloqué |
| 37 | Comptes initiaux et permissions | bootstrap admin/opérateur | ADMIN initial | provisioning de comptes dans l’API incomplet ; bootstrap sécurisé à concevoir, sans compte par défaut |
| 38 | Remise client et impression locale | document-data + génération/impression locale | agent autorisé | dépend du moteur PDF du nouveau client ; aucun fichier serveur |
| 39 | Fiche révision tarif eau | historique des révisions et motif/approbation | ADMIN/SUPERVISOR | modèle/route de création et historique d’approbation manquants |
| 40 | Matrice des droits initiaux | API de rôles/permissions versionnée | ADMIN | politique côté routeur existe partiellement ; endpoints de lecture/administration et tests écran à préciser |

#### Tests existants reliés aux parcours

Les tests ci-dessous prouvent des comportements du backend ou des API ; aucun ne valide le rendu de l’écran Windows.

| Parcours couverts | Tests existants (nom exact) | Limite constatée |
|---|---|---|
| Facture eau + électricité, aperçu et 0 m³ | `utility_preview_returns_both_services_without_persisting_financial_data` (`sel-local/tests/local_http.rs`); `utility_invoice_always_returns_electricity_and_zero_water_as_json` (`sel-api/tests/http_contract_red.rs`); `zero_water_is_always_visible_without_inventing_a_tariff` (`sel-domain/tests/billing_red.rs`); `utility_issue_is_atomic_idempotent_and_survives_restart` (`sel-api/tests/postgres_commercial.rs`) | Aucun test du flux du nouveau frontend jusqu’à confirmation, PDF et impression. |
| Création et consultation de clients | `local_commercial_operations_are_persisted_and_offline_pending` (`sel-local/tests/local_commercial.rs`); `customer_api_reads_from_postgres_after_new_app_state` (`sel-api/tests/postgres_customer_api.rs`); `customer_listing_supports_bounded_cursor_pagination_and_status_filter` (`sel-api/tests/http_contract_red.rs`) | Pas de validation des écrans, formulaires ni permissions précises par action UI. |
| Paiements, allocations, concurrence et corrections | `duplicate_allocations_are_aggregated_before_balance_validation`, `concurrent_payments_cannot_both_consume_the_same_balance`, `post_record_payment_allocation_is_atomic_idempotent_and_grouped` (`sel-storage/tests/postgres_payments_concurrency.rs`); `payment_sync_is_applied_before_the_central_ack_and_is_idempotent` (`sel-storage/tests/postgres_sync_financial.rs`); `invoice_void_is_audited_and_idempotent` (`sel-api/tests/postgres_void.rs`) | L’historique UI, le reçu, les contre-écritures complètes et l’affectation depuis les créances proposées restent à valider/implémenter. |
| Vente solaire, contrat, échéancier | `solar_sale_and_installment_contract_are_atomic_and_idempotent`, `concurrent_solar_contract_activation_creates_one_schedule_and_one_conflict` (`sel-api/tests/postgres_solar.rs`); `solar_sale_and_activation_use_one_receivable_and_exact_schedule` (`sel-api/tests/sync_and_solar_contract.rs`); `solar_sale_and_contract_projections_converge_idempotently` (`sel-local/tests/sync_worker.rs`) | Aucun parcours Windows catalogue→devis→vente→contrat→paiements→document n’est validé. |
| Synchronisation et reprise multi-postes | `two_sqlite_devices_converge_through_real_postgres_and_http` (`sel-local/tests/two_device_convergence.rs`); `central_acceptance_reconciles_pending_invoice_and_payment_without_duplicates`, `local_restart_recovers_in_flight_outbox_without_changing_identity`, `pulled_changes_roll_back_projection_and_cursor_after_mid_batch_failure` (`sel-local/tests/sync_worker.rs`); `sync_ack_is_durable_and_only_follows_business_commit` (`sel-api/tests/postgres_sync.rs`) | Ces preuves backend ne couvrent pas les états/alertes du nouveau client. |
| Authentification, appareil, intégrité et stockage | `individual_session_login_rotation_and_revocation_are_enforced`, `role_and_organization_boundaries_reject_write_escalation_and_idor` (`sel-api/tests/postgres_auth.rs`); `admin_can_enroll_revoke_device_and_issue_signed_offline_grant` (`sel-api/tests/postgres_device_grants.rs`); `sqlite_command_journal_and_outbox_survive_reopen` (`sel-storage/tests/sqlite_local_durability.rs`); `server_schemas_contain_no_invoice_file_storage_contract` (`sel-storage/tests/migrations_and_files.rs`); `openapi_document_is_valid_yaml_and_declares_no_pdf_route` (`sel-contracts/tests/openapi_contract.rs`) | Les rôles du tableau d’écrans restent des attentes de conception jusqu’à confirmation systématique des politiques de tous les handlers. |

La matrice d’acceptation d’implémentation devra ajouter les tests manquants et lier chaque contrôle visuel/API aux noms exacts ; la présence des tests ci-dessus n’est pas assimilée à une validation de l’écran Windows. Les suites n’ont pas été relancées pendant ce cadrage : les références sont issues de l’inventaire du code et les résultats exécutés à retenir sont ceux des workflows CI par SHA.

### Identité et bootstrap du poste

`desktop_v2.py` crée un `organization_id` et un `device_id` aléatoires lors de la création du premier fichier de config, puis les conserve. Ce choix assure une identité locale stable après le premier lancement, mais ne prouve pas l’association à l’organisation SEL centrale ni l’enrôlement par un administrateur. `sel-local` exige ces UUID comme variables d’environnement. Le nouveau flux d’activation doit lier identité, agent et appareil au central et refuser une régénération silencieuse. Le jeton du pont actuel est conservé dans la configuration locale : le futur client ne doit pas exposer un secret d’agent à JavaScript.

### Décision technique proposée pour le spike

Piste à essayer : React + TypeScript empaquetés localement dans Tauri 2. Pour limiter la surface locale, le navigateur embarqué devrait appeler une interface Tauri IPC typée ; le code natif Tauri supervise `sel-local` et porte l’accès HTTP local, sans injecter le bearer local dans le JavaScript. Comparer ce montage au lancement direct de l’agent depuis Tauri, vérifier les permissions, la fermeture propre, WebView2/runtime et installateur. **Ce montage reste une proposition tant qu’un prototype n’a pas compilé et démarré sur Windows.**

Pré-requis bloquant observé sur cette session : installer/configurer un toolchain Rust MSVC et Visual Studio C++ Build Tools ou exécuter le spike sur une machine/runner Windows qui en dispose. Ne pas installer un gros toolchain sur le poste SEL comme expérience.

## Calcul de progression

Progression phase 0 : **76,25 % (arrondie à 76 %)**. Pondération : dépôt/PR 10 % (validé) ; CI 5 % (100 % acquis, 4/4 workflows verts sur le SHA de code `ee62737`) ; lecture des consignes et préséance 10 % (validé) ; inventaire Figma 20 % (validé) ; cartographie écrans/API/tests 25 % (95 % acquis : 40 écrans reliés aux contrats et gaps, tests backend reliés par parcours ; permissions exactes par handler restent à vérifier) ; identification du paquet livré 10 % (75 % acquis : dernière release publique identifiée, installation SEL inconnue) ; spike Windows 20 % (0 %, bloqué par toolchain manquant). Calcul : 10 + 5 + 10 + 20 + 23,75 + 7,5 = **76,25 %**. Les éléments bloqués ne sont pas comptés comme terminés.

## Actions restantes avant clôture de phase 0

1. Vérifier les politiques RBAC handler par handler et figer les droits réels du tableau écran/API.
2. Produire un environnement Windows de développement avec Rust MSVC et Build Tools, puis construire/lancer le spike Tauri↔agent local ; éprouver WebView2, installateur et démarrage. Ce point reste `BLOCKED_EXTERNAL` dans l’environnement actuel.
3. Obtenir une preuve de la version installée/livrée au poste SEL, ou garder explicitement `BLOCKED_EXTERNAL` jusqu’à réception d’un inventaire ou d’un reçu.
4. Finaliser l’ADR du shell à partir du résultat du spike. En attendant, Tauri 2 + React/TypeScript reste une option candidate, pas une décision validée.
