# SEL V2 Backend — rapport d’implémentation

Date de contrôle : 3 novembre 2026, Africa/Lubumbashi

Branche : `monarque/sel-v2-backend`

PR : [#58](https://github.com/jupi-z/SEL-Facturation-Desktop/pull/58)

Commit contrôlé : `3f03e57` (`docs(v2): record migration integration evidence`)
Périmètre : backend Rust/API, PostgreSQL, SQLite locale, synchronisation et LiveUpdate. Aucun nouveau frontend, aucune fusion dans `main`, aucune release et aucun déploiement réel.

## Mise à jour du contrôle — avoirs append-only

Les commits `dd76760` à `0539129` ajoutent et stabilisent le parcours d'avoir métier sans réécrire les factures ni les paiements historiques :

- `POST /v1/receivables/{id}/credit` côté PostgreSQL, protégé par authentification, rôle `ADMIN`/`SUPERVISOR`, clé d'idempotence et verrouillage de la créance ;
- `POST /local/v1/receivables/{id}/credit` côté SQLite, avec commande durable, audit et outbox atomiques ;
- table `credit_notes` dans PostgreSQL et SQLite, avec montant positif, motif obligatoire et identité d'acteur ;
- soldes et états de créance incluant les crédits (`credited`, `CREDITED`, `SETTLED`) ;
- publication `credit_notes` et projection SQLite idempotente, avec conflit durable si l'entité n'est pas compatible ;
- tests de création, idempotence HTTP centrale, persistance locale, rejeu de projection et absence de doublon ;
- lecture des agrégats SQLite sans dépendre du type dynamique `REAL`/`TEXT` et conservation de l'état `CREDITED` dans les résumés local et central.

Les remboursements externes impliquant un moyen de paiement hors système restent volontairement distincts : ils nécessitent une règle métier SEL et un rapprochement opérationnel qui ne sont pas définis dans les documents disponibles.

## Mise à jour du contrôle — 3 novembre 2026

Les corrections cumulées pendant la reprise de la PR #58 ferment plusieurs régressions détectées :

- la clé de grant offline exigée au démarrage doit être une vraie clé publique Ed25519 décodable ;
- `created_at` est maintenant renseigné dans SQLite pour l’écriture et la projection d’une annulation de facture ;
- le calcul PostgreSQL des montants échus est syntaxiquement correct et couvert par le test de persistance ;
- les timestamps du format canonique des grants sont normalisés à la microseconde, afin que la signature reste stable après stockage PostgreSQL.
- le statut `/local/v1/sync/status` distingue désormais une identité offline configurée d'une absence de grant, au lieu de retourner systématiquement `NOT_CONFIGURED` ;
- l'affectation post-enregistrement est maintenant disponible hors ligne via `POST /local/v1/payments/{id}/allocate`, avec journal/outbox atomiques, idempotence persistante et recalcul des soldes après redémarrage ;
- les agrégats monétaires SQLite de ce parcours sont convertis explicitement en texte avant parsing décimal, afin de neutraliser les types dynamiques `REAL`/`TEXT` ;
- le rejeu central d'un événement `PAYMENT_ALLOCATED` est maintenant couvert par un test PostgreSQL réel, avec complément d'affectation, publication centrale unique par événement et accusé idempotent.
- les affectations locales en attente adoptent l'identifiant central lorsqu'une projection confirmée représente la même affectation, au lieu de créer une deuxième ligne ;
- le verrou PostgreSQL de rejeu est conservé jusqu'à l'accusé durable : deux transmissions simultanées du même événement donnent exactement un accepté et un doublon ;
- la facturation eau positive avec tarif et taxe approuvés est maintenant exercée dans le test PostgreSQL réel, en complément du cas obligatoire à 0 m³.
- une course PostgreSQL de deux émissions sur le même abonné et la même période est maintenant couverte : une seule facture est créée et la seconde tentative retourne `InvoicePeriodExists` (conflit métier, pas erreur 500) ;
- le résumé d’une créance solaire expose maintenant une preuve dédiée des échéances échues : le montant passé impayé est calculé, puis disparaît après affectation du paiement à l’échéance concernée.
- la liste des clients dispose maintenant d'une pagination par curseur bornée à 100 éléments et d'un filtre de statut, tout en conservant la réponse JSON historique ;
- un test PostgreSQL de concurrence couvre deux activations simultanées d'un échéancier solaire : une seule création est acceptée et l'autre est rejetée explicitement comme contrat déjà existant ;
- l'application atomique d'un lot de changements SQLite est maintenant testée sous erreur au milieu du lot : la projection déjà commencée et le curseur sont annulés ensemble, puis le rejeu valide converge.
- la canonisation de l'empreinte trie les lignes de service par clé stable ; une inversion accidentelle des lignes électricité/eau ne crée donc pas un faux conflit financier.
- les commandes financières synchronisées utilisent maintenant le même contexte transactionnel PostgreSQL pour l'effet métier et `sync_inbox` : facture eau + électricité, paiement, affectation, vente solaire, contrat solaire, annulation, contre-écriture et avoir ; cette couverture est confirmée par la CI du commit contrôlé.
- la CI du commit contrôlé `a3063b5` confirme le refactor transactionnel, le CLI d'exploitation, le garde-fou de collision de séquence, les routes de référentiels et la compilation du rapprochement V1→V2 : formatage, Clippy `-D warnings`, tests workspace et tests PostgreSQL de persistance/concurrence sont tous verts dans le run `41642740720`.
- le CLI expose désormais `backup-postgres` et `restore-postgres` : le restore est fail-closed sans `SEL_RESTORE_APPROVED=I_UNDERSTAND`, exige une sauvegarde pré-restauration distincte et utilise `pg_restore --single-transaction --exit-on-error` ; la CI `41642740720` valide le code et le test d'approbation, mais aucun restore réel n'est exécuté sur une base SEL.
- la réception SQLite des changements centraux refuse désormais les collisions de séquence avec un contenu différent, les trous antérieurs au curseur durable et tout recul du curseur ; le lot est annulé sans modifier la projection ni le curseur, avec un test de rejeu conflictuel dédié.
- les référentiels centraux disposent maintenant de routes de lecture protégées pour les produits actifs, les tarifs approuvés et datés, les profils fiscaux approuvés et les taux de change approuvés ; les montants sont renvoyés sous forme de chaînes décimales canoniques et un test HTTP PostgreSQL vérifie l'exclusion des brouillons, révocations et éléments inactifs.
- `sel-cli reconcile <portable-v1.json>` compare maintenant en lecture seule les identifiants stables, la présence des clients/factures/paiements et les totaux financiers entre un export V1 et PostgreSQL ; il renvoie `MATCH` ou `MISMATCH`, `writes: 0`, l'empreinte de la source et les écarts détaillés.
- `GET /v1/installment-plans/{id}` expose maintenant l'échéancier solaire persistant, ses échéances, les paiements acceptés, les soldes et les retards, avec filtrage par organisation ; le test HTTP PostgreSQL vérifie la lecture après activation du contrat.
- `GET /local/v1/installment-plans/{id}` expose le même état depuis SQLite, y compris les paiements locaux en attente, afin que le parcours reste consultable hors connexion ; le test commercial local vérifie la lecture après activation.
- le chemin `migrate-v1 --apply --approved-manifest <manifest.json>` transmet maintenant le vrai chemin du manifeste ; le test de parsing dédié est vert.
- le test PostgreSQL réel d'import V1 a détecté puis corrigé l'inversion des paramètres SQL : `subtotal` reçoit le décimal et `tax_snapshot` le JSONB attendu par PostgreSQL.
- le test de migration réel vérifie l'import sur PostgreSQL 16.4, le dump de sauvegarde, puis le rejeu du même manifeste avec `ALREADY_APPLIED` et zéro écriture supplémentaire.

La CI du commit contrôlé est verte : [run — 41649833616](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41649833616), job `rust` `125964745208`, sur `3f03e57`. Les contrôles de formatage, Clippy, tests workspace et tests PostgreSQL ont tous conclu avec succès, y compris l'import V1 transactionnel et idempotent sur fixture synthétique.

## Audit des branches GitHub

L'inventaire distant du 3 novembre 2026 compte `main`, `monarque/sel-v2-backend` et dix branches historiques supplémentaires. Les branches ci-dessous ne sont pas supprimées automatiquement, car elles contiennent encore des modifications non intégrées à `main` ou à la V2 :

| Branche | État vérifié | Décision contrôlée |
|---|---|---|
| `monarque/water-electricity-crud` | PR #47 ouverte, 9 commits en avance sur `main` | à préserver jusqu'à décision de fusion V1 ou d'abandon |
| `monarque/installer-webview2-resilience` | PR #54 ouverte, 10 commits en avance sur `main` | à préserver ; dépendances Windows et installateur non repris dans cette PR backend |
| `monarque/new-invoice-form-layout` | PR #55 ouverte, 12 commits en avance sur `main` | à préserver ; correctifs UI V1 non repris dans la V2 |
| `monarque/combined-invoice-water-electricity` | 13 commits en avance, preuves PDF et moteur V1 | à préserver jusqu'à consolidation de la livraison V1 |
| `monarque/sel-safeupdate-v1` | 3 commits en avance, code SafeUpdate et documentation | à préserver ; clé de production et validation native restent externes |
| autres branches UI/CI historiques | diffs encore distincts de `main` | à archiver ou supprimer uniquement après confirmation de perte acceptable |

Conclusion : aucun travail de branche n'est silencieusement considéré comme intégré. L'objectif de deux branches reste documenté, mais la suppression des branches historiques est reportée tant que leur contenu utile n'est pas consolidé et que les PR ouvertes ne sont pas explicitement clôturées.

## Résumé exécutif

Le backend Rust dispose maintenant d’un parcours démontré de bout en bout pour une facture eau + électricité et un paiement partiel : deux bases SQLite indépendantes créent et conservent leurs identifiants hors ligne, un serveur HTTP Rust persiste les commandes dans PostgreSQL, puis le second poste récupère les mêmes données. Le scénario comprend aussi une vente de kit solaire, son échéancier, une reprise après redémarrage et une tentative de double encaissement.

Le correctif de canonisation (`358ba74`) unifie l’empreinte des factures. Le format canonique est versionné, exclut les statuts de synchronisation et encode les montants avec six décimales fixes, ce qui neutralise la différence d’échelle entre SQLite (`0.095`) et PostgreSQL (`0.095000`). Un test de contrat vérifie explicitement cette invariance.

La validation du correctif actuel `3f03e57` est suivie ci-dessus. Cette exécution distante valide notamment le rapprochement V1→V2 en lecture seule, la pagination/filtre clients, les référentiels actifs/approuvés, le test RBAC/IDOR, la concurrence solaire, la lecture centrale et locale d'un échéancier persistant, le rollback atomique d'un pull SQLite, la canonicalisation de l'empreinte, les transactions de synchronisation financière, le rejet des collisions de séquence, les garde-fous de restauration et l'import V1 réel sur fixture synthétique avec rejeu idempotent. L’environnement Rust local de ce poste ne dispose pas de `link.exe`, donc la compilation et l'exécution locales complètes restent non disponibles. Cela ne remplace pas une recette Windows, une restauration contrôlée ni une validation métier. Le backend reste **NO-GO pour une exploitation commerciale** : les contrôles IDOR/concurrence exhaustifs, la sauvegarde/restauration PostgreSQL réelle, certaines opérations comptables et la recette sur poste SEL restent non validés ou incomplets.

## Corrections réalisées sur ce jalon

- Empreintes de factures déterministes et partagées entre SQLite, API et PostgreSQL.
- Canonicalisation des `Decimal` à six décimales dans le snapshot versionné ; les champs financiers sont distincts des statuts `LOCAL_PENDING`/`CENTRAL_ACCEPTED`.
- Réconciliation d’une facture locale confirmée centralement sans réécrire ses identifiants, lignes, montants ou snapshot.
- Réconciliation idempotente des paiements et de leurs affectations, sans second paiement.
- Contre-écritures de paiements append-only : renversement partiel ou total, verrouillage du paiement, somme cumulée bornée, acteur actif vérifié, audit et `central_changes` dans la même transaction.
- Avoirs append-only sur créance : montant borné par le solde, motif obligatoire, rôles autorisés, audit, idempotence centrale, outbox locale et projection sans doublon.
- Soldes PostgreSQL et SQLite calculés sur les allocations nettes après renversements, avec rejeu idempotent de l’événement `PAYMENT_REVERSED` et projection locale contrôlée.
- Import V1 → PostgreSQL désormais implémenté derrière `migrate-v1 --apply --approved-manifest` : sauvegarde `pg_dump` obligatoire, approbation explicite, UUID v5 stables, `import_batches`, `legacy_id_map`, transaction et rejeu par checksum ; les historiques restent mono-service, sans ligne eau inventée. Le test CI réel vérifie aussi l'import, le dump de sauvegarde et le rejeu idempotent ; les paramètres `subtotal`/`tax_snapshot` sont alignés sur les types PostgreSQL.
- Outils d'exploitation PostgreSQL : `backup-postgres <backup.dump>` produit un dump custom et son SHA-256 ; `restore-postgres <backup.dump>` impose une approbation explicite et une sauvegarde de sécurité avant restauration.
- Validation globale des affectations par créance, verrouillage PostgreSQL déterministe et rejet d’une surallocation, y compris deux affectations répétées vers la même créance.
- Affectation post-enregistrement `POST /v1/payments/{id}/allocate` : verrouillage du paiement et des créances, regroupement des lignes répétées, plafond sur le reçu et la créance, rejeu idempotent, audit et projection `payments` centrale atomiques ; tests API et PostgreSQL ajoutés.
- Parité locale `POST /local/v1/payments/{id}/allocate` : encaissement partiellement affecté puis complément affecté hors ligne, répétition idempotente, outbox/audit atomiques et persistance vérifiée après réouverture SQLite.
- Publication centrale récupérable après commit métier avant accusé, avec outbox/replay idempotent.
- Projections SQLite solaires : vente, lignes, créance, contrat, plan et échéances avec identifiants stables et relations conservées.
- Entités centrales non supportées par une projection locale enregistrées comme conflits durables au lieu d’être ignorées silencieusement.
- Facture de services toujours composée des deux lignes : électricité en kWh et eau en m³ ; `0 m³` est conservé et visible.
- Rejet explicite des effets locaux refusés, sans suppression destructive de l’audit ou de l’outbox.
- Statut local honnête : une identité acteur/grant persistée est affichée `CONFIGURED`; l'API sans `LocalStore` reste explicitement `NOT_CONFIGURED`.

## Preuve décisive de convergence

Test : `v2/crates/sel-local/tests/two_device_convergence.rs`
Commande exécutée par la CI avec PostgreSQL 16.4 réel :

```text
TEST_DATABASE_URL=postgres://sel:postgres@127.0.0.1:5432/sel_v2_test
cargo test --workspace -- --ignored
test two_sqlite_devices_converge_through_real_postgres_and_http ... ok
test result: ok. 1 passed; 0 failed
```

La commande n’a pas été exécutée localement dans cet environnement : la compilation MSVC s'arrête faute de `link.exe`. La preuve reproductible disponible est donc la CI GitHub `41642740720` ci-dessous, et non une prétendue exécution locale.

Le scénario vérifie :

1. client, facture combinée et paiement partiel créés sur SQLite 1 ;
2. réouverture de SQLite 1 avant synchronisation ;
3. serveur HTTP réel `sel-api` avec repository PostgreSQL réel ;
4. persistance centrale puis accusé ;
5. récupération des données sur SQLite 2 ;
6. égalité des identifiants, lignes, totaux et empreintes ;
7. vente solaire, une créance, un contrat et deux échéances ;
8. rejeu idempotent des projections solaires ;
9. deux tentatives de paiement de 70 USD sur une créance de 120 USD : une seule est acceptée, la seconde est `REJECTED`, le solde reste 50 USD ;
10. absence de double comptabilisation dans PostgreSQL et SQLite 2.

Ce test ne constitue pas encore une simulation exhaustive de panne physique à chaque instruction réseau ni une concurrence multi-processus réellement simultanée ; ces points restent marqués comme non testés ci-dessous.

## Résultats de validation

### Exécution locale

- Suite Rust complète locale : **NOT_EXECUTED** ; la compilation MSVC locale est bloquée par l'absence de `link.exe`/Build Tools.
- Tests du commit contrôlé `3f03e57` : **PASS dans la CI GitHub**, avec PostgreSQL 16.4 de service.
- Vérification locale de formatage : **PASS** ; tentative locale de compilation : **BLOCKED_EXTERNAL** par l'absence de `link.exe`.
- Relecture statique du diff et vérification `git status` : **PASS** ; aucun secret, PDF, image ou facture binaire n’a été ajouté au serveur par les changements de ce jalon.

### CI GitHub

Run [41649833616](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41649833616), job `rust` `125964745208` : **SUCCESS** sur `3f03e57`.

- `cargo fmt --all -- --check` : **PASS** ;
- `cargo clippy --workspace --all-targets -- -D warnings` : **PASS** ;
- `cargo test --workspace` : **PASS** ;
- `cargo test --workspace -- --ignored` avec PostgreSQL 16.4 : **PASS** ;
- test `two_sqlite_devices_converge_through_real_postgres_and_http` : **PASS après correction du double comptage d’affectation** ;
- test `migration_apply_is_transactional_and_idempotent_on_postgres` : **PASS**, avec fixture V1, sauvegarde `pg_dump`, import transactionnel et rejeu sans écriture ;
- test `concurrent_utility_invoices_same_customer_period_return_one_business_conflict` : **PASS dans la CI PostgreSQL du commit contrôlé** ;
- test `receivable_summary_reports_only_unpaid_past_due_installments` : **PASS dans la CI PostgreSQL du commit contrôlé** ;
- test local `local_commercial_operations_are_persisted_and_offline_pending` avec affectation post-enregistrement et rejeu idempotent : **PASS** ;
- test local `local_solar_sale_and_contract_are_persisted_offline` : **PASS** ;
- test PostgreSQL `payment_sync_is_applied_before_the_central_ack_and_is_idempotent` avec événement `PAYMENT_ALLOCATED` séparé : **PASS** ;
- tests local et PostgreSQL des avoirs append-only, de l’idempotence, de la projection sans doublon et de la conservation du statut `CREDITED` : **PASS** ;
- nettoyage du conteneur PostgreSQL de CI : **PASS**.

La CI est une validation Linux/PostgreSQL de test. Elle ne remplace pas la recette Windows, l’impression ni la validation d’un serveur SEL.

## Matrice de recette

`PASS` signifie que le périmètre exact est démontré. `PARTIAL` signifie qu’une partie seulement est prouvée. `NOT_TESTED` signifie qu’une preuve dédiée manque. `BLOCKED_EXTERNAL` nécessite une donnée, une clé, un serveur ou une validation externe.

| ID | Statut | Preuve ou limite |
|---|---|---|
| API-01 | PASS | routes commerciales centrales testées via Axum + PostgreSQL réel |
| API-02 | PARTIAL | OpenAPI valide avec l’affectation post-enregistrement ajoutée et tests ciblés ; couverture exhaustive de tous les DTO/routes à industrialiser |
| API-03 | PARTIAL | sessions Argon2id, rotation/révocation, enrôlement/révocation d'appareils, grants Ed25519 et test RBAC/IDOR ajoutés ; couverture exhaustive des ressources encore incomplète |
| API-04 | PARTIAL | création/lecture/modification, pagination par curseur bornée et filtre de statut ajoutés et validés en CI ; autres parcours CRUD spécialisés encore incomplets |
| API-05 | PASS limité | idempotence facture, paiement, solaire et sync démontrée ; concurrence multi-processus non exhaustive |
| API-06 | PASS | aucune route PDF ; `document-data` JSON côté client ; aucun stockage de fichier |
| FIN-01 | PASS | facture électrique + eau, `0 m³` visible, Decimal et arrondi testés |
| FIN-02 | PASS | eau positive refusée sans tarif/taxe validés ; eau nulle conservée |
| FIN-03 | PASS limité | test PostgreSQL réel avec tarif eau 1,25 USD/m³, taxe approuvée, 2 m³ et ligne calculée ; la recette métier SEL du tarif reste externe |
| FIN-04 | PASS | paiement partiel, solde et retry sans doublon |
| FIN-05 | PASS limité | vente solaire, créance, contrat et deux échéances persistés et projetés |
| FIN-06 | PASS limité | preuve dédiée ajoutée sur le commit contrôlé ; montant échu et statut `OVERDUE` sont vérifiés sur une échéance passée impayée, puis remis à zéro après affectation ; tableau complet de recouvrement métier reste à valider |
| FIN-07 | PARTIAL | contre-écritures de paiements append-only et affectation post-enregistrement auditable, solde net et rejeu testés ; remboursements externes et leur rapprochement restent à finaliser |
| FIN-08 | PARTIAL | annulation de facture existante et avoir append-only central/local avec projection idempotente sont testés ; remboursement externe et compensation inter-créances restent dépendants d'une règle métier SEL explicite |
| FIN-09 | PARTIAL | Decimal et snapshot multi-store ; matrice complète USD/CDF et taux approuvés à compléter |
| FIN-10 | PARTIAL | tarif utility approuvé/daté et produit solaire actif contrôlés ; matrice exhaustive des catégories à compléter |
| SYN-01 | PASS | test deux SQLite + PostgreSQL réel, reprise après ouverture et projections idempotentes |
| SYN-02 | PARTIAL | retry/replay et rejeu concurrent du même événement démontrés ; coupure injectée à chaque frontière réseau non exhaustive |
| SYN-03 | PASS limité | les commandes financières synchronisées et leur accusé sont regroupés dans une transaction PostgreSQL sur le code courant ; publication/récupération après commit simulé couvertes, mais panne physique réelle non injectée |
| SYN-04 | PARTIAL | pull et curseur durable testés ; crash SQLite avant commit à compléter |
| SYN-05 | PASS limité | deux connexions PostgreSQL concurrentes sur le même abonné/période donnent une création et un conflit `InvoicePeriodExists`, une seule ligne persistée ; scénario Windows multi-postes non réalisé |
| SYN-06 | PARTIAL | surallocation centrale et rejeu concurrent protégés ; paiements réellement simultanés sur une même créance entre deux postes à compléter |
| SYN-07 | PARTIAL | la course d'activation d'un échéancier solaire est maintenant couverte ; les conflits multi-postes de modification de contrat restent à valider |
| SYN-08 | PASS limité | émission/révocation de devices, grants Ed25519, expiration, scopes et limites d’événements testés sur PostgreSQL réel ; provisionnement de la clé de production et révocation opérationnelle restent externes |
| SYN-09 | PASS limité | worker push/pull et reprise de l’outbox codés ; réseau réellement intermittent non validé sur Windows |
| SYN-10 | PASS | Le redémarrage local récupère les événements `IN_FLIGHT` en `PENDING` sans changer les identifiants persistés ; test `local_restart_recovers_in_flight_outbox_without_changing_identity`. |
| UPD-01 | PASS | manifeste LiveUpdate déclaratif signé sans remplacement natif |
| UPD-02 | PASS | signature Ed25519, SHA-256, expiration, séquence et chemins invalides rejetés |
| UPD-03 | PASS | activation et rollback connu-bon testés |
| UPD-04 | NOT_TESTED | SafeUpdate natif hors périmètre de cette validation backend |
| UPD-05 | BLOCKED_EXTERNAL | clé publique Ed25519 de production absente ; fail-closed conservé |
| MIG-01 | PASS limité | dry-run et import transactionnel vérifiés ; fixture synthétique PostgreSQL avec `pg_dump` exécutée en CI `41649126504` |
| MIG-02 | PASS limité | checksum `import_batches`, mapping UUID, import PostgreSQL et rejeu idempotent vérifiés : second passage `ALREADY_APPLIED`, `writes: 0` ; données V1 SEL anonymisées encore absentes |
| MIG-03 | BLOCKED_EXTERNAL | données V1 anonymisées de rapprochement absentes |
| MIG-04 | BLOCKED_EXTERNAL | campagne reprise/rollback d’import non réalisée |
| MIG-05 | PARTIAL | commandes de sauvegarde/restauration contrôlées, rapprochement lecture seule et test d'approbation fail-closed livrés ; restauration réelle et exécution du rapprochement sur une base de test/prod restent à faire |
| FILE-01 | PASS | serveur/API ne déclarent ni ne stockent de PDF |
| FILE-02 | PASS | schémas et tests ne contiennent pas de champ de fichier de facture |
| FILE-03 | NOT_TESTED | audit des volumes/logs/backups après charge non réalisé |
| FILE-04 | NOT_TESTED | générateur PDF client V2 volontairement hors backend |
| OPS-01 | PASS | formatage, Clippy, tests workspace et tests PostgreSQL ignorés verts en CI `41642740720` sur `a3063b5` |
| OPS-02 | PASS | PostgreSQL de test en CI avec migrations et tests ignorés verts |
| OPS-03 | NOT_TESTED | TLS, reverse proxy, PII, sauvegardes chiffrées et moindre privilège non validés |
| OPS-04 | BLOCKED_EXTERNAL | poste Windows SEL, impression et serveur réel non accessibles pour recette |

## Lacunes et décisions restant à traiter

1. Finaliser l’enrôlement d’appareils, la révocation des grants et les tests IDOR/RBAC/concurrence de production.
2. Définir avec SEL la règle de remboursement externe et la compensation inter-créances ; l'avoir append-only est livré, mais aucun remboursement réel ne doit être inventé sans ce cadrage.
3. Exécuter le nouvel import V1 sur une base PostgreSQL de test avec `pg_dump` disponible, vérifier le rapprochement et la reprise sur données anonymisées fournies par SEL.
4. Documenter et exercer la sauvegarde/restauration PostgreSQL, TLS, rotation des secrets et le déploiement à privilèges minimaux.
5. Provisionner la clé publique Ed25519 de production hors dépôt avant d’activer LiveUpdate ; aucune signature de test ne doit être promue.
6. Réaliser la recette sur un PC Windows SEL : installation, fonctionnement hors connexion, synchronisation, impression, sauvegarde/restauration et validation métier des tarifs eau/taxes.

## Verdicts

- Revue technique du jalon persistance/synchronisation : **GO limité** ; la CI `41649833616` est verte sur `3f03e57`, avec l'import V1 synthétique idempotent, les tests PostgreSQL/SQLite, les référentiels actifs/approuvés, les accusés de synchronisation financiers regroupés transactionnellement et le rejet des collisions de séquence.
- Pilote Windows/PostgreSQL/offline : **NO-GO** ; validation matérielle et opérationnelle non réalisée.
- Livraison commerciale : **NO-GO** ; sécurité de production, migration, exploitation et opérations comptables restantes.
- Fusion dans `main` : **NON EFFECTUÉE**.
- Release, publication d’installateur et déploiement réel : **NON EFFECTUÉS**.

La PR #58 est prête pour revue technique sur le périmètre démontré, mais ne doit pas être annoncée comme backend de production tant que les statuts `PARTIAL`, `NOT_TESTED` et `BLOCKED_EXTERNAL` critiques ne sont pas fermés avec des preuves correspondantes.
