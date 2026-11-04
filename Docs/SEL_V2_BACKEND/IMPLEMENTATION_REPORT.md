# SEL V2 Backend — rapport d’implémentation

Date de contrôle : 3 novembre 2026, Africa/Lubumbashi

Branche : `monarque/sel-v2-backend`

PR : [#58](https://github.com/jupi-z/SEL-Facturation-Desktop/pull/58)

Commit de référence à la rédaction du plan : `b051eb0c48aa7c1b10d8874c3715770195145077`. Vérifier HEAD distant avant toute nouvelle décision.
Périmètre : backend Rust/API, PostgreSQL, SQLite locale, synchronisation, LiveUpdate, client Windows V2 et exploitation. Aucune fusion ou aucun déploiement n’est effectué par cette mise à jour documentaire.

## Plan de finalisation et règles de travail

- [Plan complet backend + frontend](PLAN_FINALISATION_BACKEND_FRONTEND.md) : phases, critères de sortie, difficulté et conditions de fusion autorisée.
- [Règles de travail V2](REGLES_DE_TRAVAIL_V2.md) : vérification d’état, protection métier, cycle des incréments, Git, validation et définition de terminé.
- Le propriétaire a autorisé la fusion dans `main` lorsque le système sera jugé stable et suffisamment fonctionnel. Les conditions objectives de fusion sont décrites dans le plan ; cette autorisation ne vaut pas déploiement ni migration réelle.

## Revue initiale du nouveau frontend Figma — 3 novembre 2026

État de cette revue : **inventaire Figma et vérification visuelle d’écrans représentatifs effectués pour préparer le plan ; aucun écran n’a été implémenté ou validé sur Windows dans cet incrément documentaire.**

- Fichier examiné : « SEL V2 — Product Design · Windows - Facturation » (`EuIpbci6IR7m6ggApo5WWm`). Pages repérées : Vision, Fondations, Composants, Parcours, Opérations Windows, Solaire/finance, Administration/système, Documents A4, Livraison et États/accessibilité.
- Captures examinées : tableau de bord, prévisualisation de facture et spécimen A4. Les maquettes sont desktop 1440×900 et les documents A4 portrait ; identité bleu nuit/bleu d’action avec accent jaune, typographie Geist, états textuels, navigation latérale et données de prototype clairement fictives.
- La page de livraison annonce 30 écrans métier + 4 documents A4 ; l’addendum du 3 novembre annonce 10 écrans additionnels et un cinquième modèle A4. L’inventaire des frames contient aussi des variantes et écrans additionnels : la liste de livraison devra être rapprochée de tous les frames avant d’affirmer une couverture exhaustive.
- Décision de cadrage documentée : construire un frontend neuf, sans porter l’ancienne interface. Tauri 2 + React/TypeScript est une architecture candidate, conditionnée à un spike de démarrage de l’agent local, transport UI↔Rust, dépendances WebView2 et installateur Windows. Aucun choix n’est présenté comme testé.
- Les dépendances d’API inscrites dans Figma (dont relevés, journal de caisse, reporting, conflits, administration et sauvegardes) sont des pistes de mapping ; elles doivent être confrontées aux routes et permissions réelles du backend.

Voir les sections « Relevé Figma » et « Décision de cadrage du nouveau frontend » du [plan](PLAN_FINALISATION_BACKEND_FRONTEND.md), ainsi que les règles de travail révisées. Le frontend historique n’est plus considéré comme base de réutilisation.

## Préparation de la phase 0

Contrôle initial de cette progression le 3 novembre 2026 sur le SHA `77d1e0983db26c0392ec12364663a299b4a81249`. HEAD courant contrôlé : `cc9665a5d034dbf5f2d889a79deee95ee48e51ca`, branche alignée avec son upstream, PR #58 ouverte vers `main`, CI 4/4 verte. L’audit Figma, la cartographie, puis le spike Windows local ont été réalisés. Aucun résultat de recette sur le poste SEL n’est présumé.

L’audit de phase 0 a établi que Figma comprend 40 écrans métier et 5 modèles A4, relié les écrans aux contrats et répertorié les tests backend/gaps. Un spike temporaire a ensuite compilé React/TypeScript + Tauri 2 en dev et release sous Windows 11, interrogé `sel-local` via IPC Rust authentifié, confirmé le statut `LOCAL_READY`, SQLite d’essai, arrêt du processus et produit des bundles MSI/NSIS. Les bundles ne sont pas installés ni utilisables comme produit : l’agent n’est pas empaqueté et la configuration dépend de variables d’essai. La dernière release V1 publique est identifiée, mais le paquet installé chez SEL reste inconnu. Phase 0 de cadrage clôturée à **92 %** (92,25 % détaillés) selon les pondérations du [rapport détaillé](PHASE_0_ETAT_DES_LIEUX.md). Tauri + React est retenu comme base de travail phase 1, sous réserve de valider le packaging complet de l’agent et WebView2. Les SHA-256 des trois artefacts temporaires figurent dans le rapport de phase 0 ; ils ne doivent pas être redistribués.

## Incrément client V2 — aperçu contrôlé et rendu sûr

Commit : fc41cec (feat(v2): secure client rendering and invoice preview flow)

Statut de cet incrément : **COMPLETED côté code, tests locaux et CI Rust/Windows**. La validation sur le poste SEL reste externe.

- les listes clients, factures, paiements, ventes solaires, métriques, synchronisation et options sont maintenant construites par DOM (textContent/nœuds), sans interpolation innerHTML de données métier ;
- le pont pywebview refuse les méthodes autres que GET, POST et PATCH, en plus de limiter les routes à /local/v1/ ;
- POST /local/v1/billing/utility/preview calcule une facture combinée eau + électricité sans écrire de facture, créance ou événement d'outbox ;
- l’écran de facturation affiche les deux lignes, y compris l’eau à 0 m³, les prix, sous-totaux, TVA et total avant confirmation explicite ;
- l’émission définitive reste séparée sur POST /local/v1/billing/utility/issue et réutilise le même calcul Rust que l’aperçu ;
- toute modification du formulaire après prévisualisation invalide l’aperçu afin d’empêcher la confirmation d’un calcul obsolète ;
- le test HTTP local couvre l’aperçu à 0 m³ et vérifie l’absence d’écriture financière ; les tests Python couvrent le rendu sûr, la confirmation explicite et le filtrage du pont.

Preuves exécutées localement après le commit :

- py -3.12 -m pytest -q : **267 passed** en 125,25 s ;
- py -3.12 -m pytest -q tests/test_desktop_v2.py tests/test_desktop.py : **17 passed** ;
- node --check v2/client/app.js : **PASS** ;
- cargo fmt --all -- --check : **PASS** ;
- le test Rust sel-local de prévisualisation est ajouté ; il est **BLOCKED_EXTERNAL localement** faute de link.exe MSVC, mais la CI Windows/Rust du commit de livraison l’a exécuté avec succès.

Preuves CI du commit d’intégration :

- backend Rust : run 41690022185, job 126105843793 — fmt, Clippy, tests workspace et tests PostgreSQL : **PASS** ;
- build Windows Desktop : run 41690022095, job 126105844289 — tests unitaires/HTTP, EXE, MSI, bootstrapper et contrôles de dépendances : **PASS** ;
- candidat Windows : run 41690022151, job 126105843038 — EXE, MSI, bootstrapper, sommes SHA-256 et artefact d’acceptation : **PASS** ;
- paquet serveur : run 41690022129, job 126105843550 — build et contrôles de configuration : **PASS**.

Artefact candidat généré par la CI : SEL-Facturation-V2-Windows-5.zip, artefact GitHub 13256308078, digest du ZIP sha256:5966f31361d44ec8badc58fab124d810cadc059e9a13fb68278f18a1dfcae86a. Le paquet contient SEL-Facturation-V2-Setup-0.3.30.exe, le MSI, l’EXE et SHA256SUMS.txt. Il est non signé Authenticode et reste un candidat de test, pas une publication commerciale.

Limites encore ouvertes pour cet incrément : la validation sur un poste Windows SEL, l’impression réelle, le test WebView2 et l’exécution Rust avec PostgreSQL/SQLite réels restent distincts des tests locaux ci-dessus. Aucun calcul financier n’est effectué dans le JavaScript.

## Incrément paiements — sélection sûre des créances

Commit : 490cc4a (feat(v2): select open receivables from customer statement)

- le champ libre d’identifiant de créance est remplacé par une liste liée au client sélectionné ;
- le client local charge GET /local/v1/customers/{id}/statement et ne présente que les créances OPEN ou OVERDUE avec un solde strictement positif ;
- les changements rapides de client sont protégés contre le retour tardif d’une réponse obsolète ;
- le moteur Rust reste l’autorité pour l’affectation, les montants, les soldes et l’idempotence ;
- le test UI vérifie la présence de la sélection et du relevé local ; la suite ciblée desktop/V2 passe à 18 tests.

Preuves CI du commit 490cc4a :

- backend Rust : run 41692359758 — **PASS** ;
- build Windows Desktop : run 41692359582 — **PASS** ;
- paquet serveur : run 41692359615 — **PASS** ;
- candidat Windows : run 41692359580 — **PASS**.

Artefact candidat correspondant : SEL-Facturation-V2-Windows-7.zip, artefact GitHub 13257956654, taille 64 152 864 octets, digest ZIP sha256:ddb8d8a45a6539975e8bc8ed41ac678abc49a85e9866655fbe8b6144ff075815. Il contient SEL-Facturation-V2-Setup-0.3.30.exe, le MSI, l’EXE et SHA256SUMS.txt. Il reste non signé Authenticode et réservé aux tests.

## Mise à jour livraison urgente — candidat Windows et paquet serveur

Le commit `bcb19d0` conserve le shell natif V2, les routes locales de lecture des factures/paiements/ventes solaires, le packaging Windows et le paquet de déploiement serveur. Les validations distantes suivantes sont vertes :

- Rust V2 : [run 41675101947](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41675101947), job `126053488345` — fmt, Clippy, tests workspace et PostgreSQL ignoré : **PASS** ;
- Windows V2 : [run 41675101937](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41675101937), job `126053487180` — EXE, MSI, bootstrapper WebView2, smoke test et SHA-256 : **PASS** ;
- paquet serveur : [run 41675101941](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41675101941), job `126053487163` — interpolation Compose et build Docker sans secrets : **PASS** ;
- chaîne Windows V1 de non-régression : [run 41675101923](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41675101923), job `126053487291` — **PASS**.

Artefact Windows : `SEL-Facturation-V2-Windows-3.zip`, artefact GitHub `13251105341`, taille `64 145 404` octets, digest `sha256:5e7495e88bf5f3365d2766c51f3855c2ea9488ce74aa51e32408215ea2617672`. Il contient `SEL-Facturation-V2-Setup-0.3.30.exe`, le MSI, l’EXE et `SHA256SUMS.txt`. Le candidat est non signé Authenticode et doit rester réservé aux tests.

Le shell V2 conserve les identités et la base SQLite dans `%LOCALAPPDATA%\SEL Facturation V2`, utilise l’agent Rust embarqué et ne transmet pas le jeton local à l’interface. Les ressources HTML/CSS/JS sont incluses localement ; WebView2 est installé par le bootstrapper uniquement si nécessaire.

Le paquet serveur est préparé dans `v2/deploy/` avec Dockerfile, Compose production, TLS Caddy, création du rôle applicatif, sauvegardes/restauration documentées et validation fail-closed. Aucun serveur SEL réel, certificat TLS, secret PostgreSQL, clé de production Ed25519 ou volume financier réel n’était accessible dans cette session. Le déploiement production reste donc **BLOCKED_EXTERNAL** et n’a pas été exécuté.

**Verdict du jalon : `GO_CANDIDATE`, pas `GO_PRODUCTION`.** Le candidat Windows est techniquement assemblé et CI-validé ; la recette sur poste SEL et le déploiement central sécurisé restent obligatoires.

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
- une fixture volontairement invalide après une première facture est maintenant vérifiée en PostgreSQL réel : la transaction est annulée et aucun client ni aucune facture partielle ne subsiste.
- les routes obligatoires de création de contrat solaire sont maintenant exposées côté central (`POST /v1/solar/contracts`) et côté agent local (`POST /local/v1/solar/contracts`) ; elles réutilisent la validation de schedule, l'idempotence et les projections existantes, avec des tests HTTP central/offline.
- un devis solaire central (`POST /v1/solar/quotes`) et local (`POST /local/v1/solar/quotes`) renvoie désormais le prix du catalogue actif approuvé ; une vente ne peut plus déclarer un principal différent du prix validé, avec rejet HTTP `422` et test PostgreSQL réel ; le parcours local vérifie également le devis hors connexion.
- les tests adversariaux LiveUpdate couvrent maintenant les chemins parent, absolus POSIX/Windows, séparateurs inversés, composants vides et traversals ZIP ; les chemins non sûrs sont rejetés avant extraction, avec validation confirmée par la CI du commit contrôlé.
- le worker local est maintenant testé lorsqu'une réponse d'accusé est perdue ou illisible après transmission : l'événement revient en attente, est rejoué sans nouvelle écriture métier et reçoit un doublon idempotent, sans perte de commande.
- le scénario de convergence à deux postes transmet maintenant deux paiements concurrents vers la même créance depuis deux SQLite distinctes ; le résultat exige exactement une acceptation, un rejet explicite et zéro événement restant en attente, quel que soit le poste gagnant.
- le relevé client local `GET /local/v1/customers/{id}/statement` est maintenant couvert par le parcours HTTP commercial : il retourne toutes les créances locales du client, conserve les identifiants canoniques et expose le solde après paiement/avoir sans recalcul financier côté interface.

La CI du commit contrôlé est verte : [run — 41665371957](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41665371957), job `rust` `126019130048`, sur `caf0025`. Les contrôles de formatage, Clippy, tests workspace et tests PostgreSQL ont tous conclu avec succès, y compris le relevé client local, la reprise après perte d'accusé, les paiements concurrents multi-postes, les devis et le rejet des prix solaires divergents côté central et local, ainsi que les variantes adversariales Windows de LiveUpdate.

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

La validation du correctif actuel `caf0025` est suivie ci-dessus. Cette exécution distante valide notamment le rapprochement V1→V2 en lecture seule, la pagination/filtre clients, les référentiels actifs/approuvés, le test RBAC/IDOR, la concurrence solaire, la lecture centrale et locale d'un échéancier persistant, le relevé client local, le rollback atomique d'un pull SQLite, la canonicalisation de l'empreinte, les transactions de synchronisation financière, le rejet des collisions de séquence, les garde-fous de restauration, l'import V1 réel sur fixture synthétique avec rejeu idempotent, l'annulation complète d'une migration échouée, les routes de création de contrat solaire central/offline, le prix solaire dérivé du catalogue actif et l'erreur métier locale `422` pour un prix divergent. L'environnement Rust local de ce poste ne dispose pas de `link.exe`, donc la compilation et l'exécution locales complètes restent non disponibles. Cela ne remplace pas une recette Windows, une restauration contrôlée ni une validation métier. Le backend reste **NO-GO pour une exploitation commerciale** : les contrôles IDOR/concurrence exhaustifs, la sauvegarde/restauration PostgreSQL réelle, certaines opérations comptables et la recette sur poste SEL restent non validés ou incomplets.

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

La commande n’a pas été exécutée localement dans cet environnement : la compilation MSVC s'arrête faute de `link.exe`. La preuve reproductible disponible est donc la CI GitHub `41665371957` ci-dessous, et non une prétendue exécution locale.

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
- Tests du commit contrôlé `caf0025` : **PASS dans la CI GitHub**, avec PostgreSQL 16.4 de service.
- Vérification locale de formatage : **PASS** ; tentative locale de compilation : **BLOCKED_EXTERNAL** par l'absence de `link.exe`.
- Relecture statique du diff et vérification `git status` : **PASS** ; aucun secret, PDF, image ou facture binaire n’a été ajouté au serveur par les changements de ce jalon.

### CI GitHub

Run [41665371957](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41665371957), job `rust` `126019130048` : **SUCCESS** sur `caf0025`.

- `cargo fmt --all -- --check` : **PASS** ;
- `cargo clippy --workspace --all-targets -- -D warnings` : **PASS** ;
- `cargo test --workspace` : **PASS** ;
- `cargo test --workspace -- --ignored` avec PostgreSQL 16.4 : **PASS** ;
- test HTTP de devis solaire issu du catalogue actif et rejet d'un principal divergent : **PASS** ;
- test local du rejet HTTP `422` pour un prix solaire divergent : **PASS** ;
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
| API-02 | PARTIAL | OpenAPI valide avec affectation post-enregistrement, devis solaire et création de contrat solaire central/offline ajoutés et testés ; couverture exhaustive de tous les DTO/routes à industrialiser |
| API-03 | PARTIAL | sessions Argon2id, rotation/révocation, enrôlement/révocation d'appareils, grants Ed25519 et test RBAC/IDOR ajoutés ; couverture exhaustive des ressources encore incomplète |
| API-04 | PARTIAL | création/lecture/modification, pagination par curseur bornée et filtre de statut ajoutés et validés en CI ; autres parcours CRUD spécialisés encore incomplets |
| API-05 | PASS limité | idempotence facture, paiement, solaire et sync démontrée ; concurrence multi-processus non exhaustive |
| API-06 | PASS | aucune route PDF ; `document-data` JSON côté client ; aucun stockage de fichier |
| FIN-01 | PASS | facture électrique + eau, `0 m³` visible, Decimal et arrondi testés |
| FIN-02 | PASS | eau positive refusée sans tarif/taxe validés ; eau nulle conservée |
| FIN-03 | PASS limité | test PostgreSQL réel avec tarif eau 1,25 USD/m³, taxe approuvée, 2 m³ et ligne calculée ; la recette métier SEL du tarif reste externe |
| FIN-04 | PASS | paiement partiel, solde et retry sans doublon |
| FIN-05 | PASS limité | vente solaire, devis depuis produit actif, prix approuvé imposé, créance, contrat et deux échéances persistés et projetés |
| FIN-06 | PASS limité | preuve dédiée ajoutée sur le commit contrôlé ; montant échu et statut `OVERDUE` sont vérifiés sur une échéance passée impayée, puis remis à zéro après affectation ; tableau complet de recouvrement métier reste à valider |
| FIN-07 | PARTIAL | contre-écritures de paiements append-only et affectation post-enregistrement auditable, solde net et rejeu testés ; remboursements externes et leur rapprochement restent à finaliser |
| FIN-08 | PARTIAL | annulation de facture existante et avoir append-only central/local avec projection idempotente sont testés ; remboursement externe et compensation inter-créances restent dépendants d'une règle métier SEL explicite |
| FIN-09 | PARTIAL | Decimal et snapshot multi-store ; matrice complète USD/CDF et taux approuvés à compléter |
| FIN-10 | PARTIAL | tarif utility approuvé/daté et produit solaire actif contrôlés ; matrice exhaustive des catégories à compléter |
| SYN-01 | PASS | test deux SQLite + PostgreSQL réel, reprise après ouverture et projections idempotentes |
| SYN-02 | PASS limité | retry/replay, réponse d'accusé perdue/illisible et rejeu concurrent du même événement démontrés ; coupure injectée à chaque frontière réseau non exhaustive |
| SYN-03 | PASS limité | les commandes financières synchronisées et leur accusé sont regroupés dans une transaction PostgreSQL sur le code courant ; publication/récupération après commit simulé couvertes, mais panne physique réelle non injectée |
| SYN-04 | PARTIAL | pull et curseur durable testés ; crash SQLite avant commit à compléter |
| SYN-05 | PASS limité | deux connexions PostgreSQL concurrentes sur le même abonné/période donnent une création et un conflit `InvoicePeriodExists`, une seule ligne persistée ; scénario Windows multi-postes non réalisé |
| SYN-06 | PASS limité | deux paiements réellement concurrents depuis deux SQLite vers une même créance sont arbitrés par PostgreSQL : un seul accepté, un rejeté explicitement, deux événements conservés et aucun pending ; campagne Windows réelle et procédure d'excédent restent externes |
| SYN-07 | PARTIAL | la course d'activation d'un échéancier solaire est maintenant couverte ; les conflits multi-postes de modification de contrat restent à valider |
| SYN-08 | PASS limité | émission/révocation de devices, grants Ed25519, expiration, scopes et limites d’événements testés sur PostgreSQL réel ; provisionnement de la clé de production et révocation opérationnelle restent externes |
| SYN-09 | PASS limité | worker push/pull et reprise de l’outbox codés ; réseau réellement intermittent non validé sur Windows |
| SYN-10 | PASS | Le redémarrage local récupère les événements `IN_FLIGHT` en `PENDING` sans changer les identifiants persistés ; test `local_restart_recovers_in_flight_outbox_without_changing_identity`. |
| UPD-01 | PASS | manifeste LiveUpdate déclaratif signé sans remplacement natif |
| UPD-02 | PASS | signature Ed25519, SHA-256, expiration, séquence et chemins invalides rejetés |
| UPD-03 | PASS | activation et rollback connu-bon testés |
| UPD-04 | PASS | tests adversariaux LiveUpdate : parent, absolu POSIX/Windows, séparateurs inversés, composants vides et traversal ZIP rejetés avant extraction ; CI `41665371957` |
| UPD-05 | BLOCKED_EXTERNAL | clé publique Ed25519 de production absente ; fail-closed conservé |
| MIG-01 | PASS limité | dry-run et import transactionnel vérifiés ; fixture synthétique PostgreSQL avec `pg_dump` exécutée en CI `41649126504` |
| MIG-02 | PASS limité | checksum `import_batches`, mapping UUID, import PostgreSQL et rejeu idempotent vérifiés : second passage `ALREADY_APPLIED`, `writes: 0` ; données V1 SEL anonymisées encore absentes |
| MIG-03 | BLOCKED_EXTERNAL | données V1 anonymisées de rapprochement absentes |
| MIG-04 | PARTIAL | rollback transactionnel après erreur de fixture démontré en CI ; campagne de reprise à 50 % et validation opérationnelle restent à réaliser |
| MIG-05 | PARTIAL | commandes de sauvegarde/restauration contrôlées, rapprochement lecture seule et test d'approbation fail-closed livrés ; restauration réelle et exécution du rapprochement sur une base de test/prod restent à faire |
| FILE-01 | PASS | serveur/API ne déclarent ni ne stockent de PDF |
| FILE-02 | PASS | schémas et tests ne contiennent pas de champ de fichier de facture |
| FILE-03 | NOT_TESTED | audit des volumes/logs/backups après charge non réalisé |
| FILE-04 | NOT_TESTED | générateur PDF client V2 volontairement hors backend |
| OPS-01 | PASS | formatage, Clippy, tests workspace et tests PostgreSQL ignorés verts en CI `41665371957` sur `caf0025` |
| OPS-02 | PASS | PostgreSQL de test en CI avec migrations et tests ignorés verts |
| OPS-03 | NOT_TESTED | TLS, reverse proxy, PII, sauvegardes chiffrées et moindre privilège non validés |
| OPS-04 | BLOCKED_EXTERNAL | poste Windows SEL, impression et serveur réel non accessibles pour recette |

## Lacunes et décisions restant à traiter

1. Finaliser l’enrôlement d’appareils, la révocation des grants et les tests IDOR/RBAC/concurrence de production.
2. Définir avec SEL la règle de remboursement externe et la compensation inter-créances ; l'avoir append-only est livré, mais aucun remboursement réel ne doit être inventé sans ce cadrage.
3. Exécuter le rapprochement et la reprise à 50 % sur des données V1 anonymisées fournies par SEL ; l'import sur fixture synthétique, son rollback et son rejeu sont désormais couverts par la CI.
4. Documenter et exercer la sauvegarde/restauration PostgreSQL, TLS, rotation des secrets et le déploiement à privilèges minimaux.
5. Provisionner la clé publique Ed25519 de production hors dépôt avant d’activer LiveUpdate ; aucune signature de test ne doit être promue.
6. Réaliser la recette sur un PC Windows SEL : installation, fonctionnement hors connexion, synchronisation, impression, sauvegarde/restauration et validation métier des tarifs eau/taxes.

## Verdicts

- Revue technique du jalon persistance/synchronisation : **GO limité** ; la CI `41665371957` est verte sur `caf0025`, avec l'import V1 synthétique idempotent, son rollback transactionnel, les parcours de contrat et devis solaire, le relevé client local, les tests PostgreSQL/SQLite, la reprise après perte d'accusé, l'arbitrage concurrent de deux paiements multi-postes, les référentiels actifs/approuvés, les accusés de synchronisation financiers regroupés transactionnellement, le rejet des collisions de séquence et les contrôles adversariaux LiveUpdate.
- Pilote Windows/PostgreSQL/offline : **NO-GO** ; validation matérielle et opérationnelle non réalisée.
- Livraison commerciale : **NO-GO** ; sécurité de production, migration, exploitation et opérations comptables restantes.
- Fusion dans `main` : **NON EFFECTUÉE**.
- Release, publication d’installateur et déploiement réel : **NON EFFECTUÉS**.

La PR #58 est prête pour revue technique sur le périmètre démontré, mais ne doit pas être annoncée comme backend de production tant que les statuts `PARTIAL`, `NOT_TESTED` et `BLOCKED_EXTERNAL` critiques ne sont pas fermés avec des preuves correspondantes.

## Phase 1 — point de progression au 3 novembre 2026

Progression actuelle de la phase contrats et sécurité : **90 %** selon les critères locaux ci-dessous. Ce pourcentage ne représente pas le degré d'achèvement de SEL V2.

| Sous-critère phase 1 | Statut | Preuve actuelle |
|---|---|---|
| Autorisation explicite par route, méthode et rôle ; refus par défaut | COMPLETED côté code/test | `role_allows_request`; 5 tests unitaires `sel-api` passent, dont routes inconnues, méthodes non exposées, droits facture et rôle AUDITOR_READONLY |
| Push et plafond de rôle sur commandes offline | COMPLETED côté code/test ciblé | 1 test unitaire couvre les combinaisons métier ; `device_is_authorized` contrôle rôle agent, rôle↔commande, acteur/appareil/grant actifs, signature, scope, expiration et limites |
| Identifiants organisation/appareil/agent/grant non nuls | COMPLETED côté code/test ciblé | Parseurs central/local durcis ; tests: `production_uuid_parser_rejects_missing_or_invalid_values`, `local_identity_configuration_rejects_nil` et identité locale stable passent |
| Configuration commerciale fail-closed | PARTIAL | sessions individuelles, grant signé, UUID non nuls et nom légal non vide requis ; le jeton legacy partagé est maintenant refusé au démarrage commercial. Provisionnement initial opérationnel à compléter |
| Rotation de mot de passe auditable et révocation des sessions | CODED, test d'intégration compilé | Transaction SQL vérifie l'acteur ADMIN, change Argon2id, révoque les sessions de la cible et ajoute un audit expurgé ; `postgres_auth` compile, mais exécution PostgreSQL locale non réalisée faute de `TEST_DATABASE_URL`/service de test |
| Contrats OpenAPI et documentation RBAC | PASS contrat local | Routes login/refresh/logout, mot de passe, annulation et contre-écriture décrites ; `openapi_document_is_valid_yaml_and_declares_no_pdf_route` passe |
| Exécution PostgreSQL et CI du commit final | PASS CI | PR #58 SHA `6d4b50083309d535beced09859608d70ae2c7112` ; quatre workflows verts, dont convergence bi-postes PostgreSQL réelle. Le poste local ne disposait pas de service PostgreSQL/Docker |

Commandes locales exécutées : `cargo fmt --all -- --check` (**PASS**), `cargo test -p sel-contracts openapi_document_is_valid_yaml_and_declares_no_pdf_route` (**1 passed**), `cargo test -p sel-api` (**toutes les suites exécutables passent : API 5 + 4, HTTP 2, contrats sync 2 ; les tests PostgreSQL sont ignorés faute de base de test**), `cargo test -p sel-local` (**21 passed ; convergence PostgreSQL ignorée faute de base de test**), `cargo test -p sel-storage offline_role_tests` (**1 passed**), `cargo test -p sel-api --test postgres_auth --no-run` (**compile PASS**), `cargo clippy -p sel-api -p sel-local -p sel-storage --all-targets -- -D warnings` (**PASS**). Le test PostgreSQL ignoré n'a pas été exécuté localement. Aucun merge, release, déploiement ou changement de données réelles n'a été effectué.

Prochaine tranche : attendre et examiner les quatre workflows sur le HEAD courant, puis corriger toute régression et vérifier à nouveau la CI sur le SHA final. Les CRUD utilisateurs/appareils/grants, l'exécution PostgreSQL locale réelle et les clés/identités commerciales SEL restent des travaux ou validations distincts.

### Addendum CI — correction de la fixture de convergence

Sur le commit `0b76f993ba6f3fd67502458bbf05859117811564`, la CI Rust a échoué dans `two_sqlite_devices_converge_through_real_postgres_and_http` : l'assertion de premières commandes acceptées obtenait 2 au lieu de 3. Le diagnostic du test et de `offline_role_allows_command` a établi que sa fixture créait un acteur `BILLING_AGENT`, puis lui attribuait également l'encaissement, réservé à `ADMIN`, `CASHIER` ou `SUPERVISOR`. La règle d'autorisation n'est pas élargie ; la fixture est corrigée pour utiliser `SUPERVISOR`, rôle adapté à ce scénario combinant facturation et paiement.

Après correction, `cargo test -p sel-local --test two_device_convergence --no-run` passe localement. Le moteur Docker/PostgreSQL n'étant pas disponible dans cet environnement, le scénario réel n'a pas été rejoué localement. La CI sur `0b76f99` reste en échec ; l'intégration ne sera validée qu'après exécution de la CI sur le commit corrigé. Progression de la phase 1 ramenée à **80 %** en attendant cette preuve.

Résultat final sur le commit de code correctif `5545c3bfe7cd8b48add7f2f39f37df88297b11c3` : les quatre workflows sont **PASS** (`SEL V2 Backend` run `41720409442`, paquet serveur `41720409520`, build Windows Desktop `41720409835`, candidat Windows `41720409433`). Le workflow Rust a réussi `cargo fmt`, Clippy, `cargo test --workspace` et les tests PostgreSQL, dont `two_sqlite_devices_converge_through_real_postgres_and_http`. Le candidat Windows a compilé et téléversé ses packages d’acceptation non publiés. Le commit suivant `6d4b50083309d535beced09859608d70ae2c7112` ne contient que la mise à jour documentaire ; ses quatre workflows sont également **PASS** (`41721448310`, `41721448525`, `41721448556`, `41721448375`). Le SHA distant courant de la PR #58 est `6d4b50083309d535beced09859608d70ae2c7112` et la PR reste ouverte. Progression révisée de la phase 1 : **90 %** ; le provisionnement initial utilisable des comptes commerciaux reste à traiter. Aucun merge, aucune release publique et aucun déploiement n'ont été effectués.

### Amorçage initial ADMIN — en cours

Une commande d'administration `sel-cli bootstrap-admin <identity-ref>` a été ajoutée. Elle demande le mot de passe via un terminal avec saisie masquée, ne prend aucun secret en argument ou variable d'environnement, ne crée pas d'organisation et n'exécute pas de migration ; elle crée un seul ADMIN et son audit de manière atomique dans une organisation déjà provisionnée. Le verrou de ligne d'organisation empêche deux premières créations concurrentes ; toute organisation ayant déjà un utilisateur est refusée. Le test PostgreSQL `initial_admin_bootstrap_is_atomic_one_time_and_authenticates` couvre deux amorçages simultanés, l'authentification du gagnant, l'unicité du compte et l'absence de secret dans l'audit. Les tests locaux du CLI/API/storage et Clippy sont PASS ; le test PostgreSQL réel est PASS dans la CI du commit `cc33c498ecce055dd17e92f7e3040c0851a3a5c2`. Les quatre workflows de ce commit sont PASS : backend `41725792419`, paquet serveur `41725792410`, build Windows `41725792406`, candidat Windows `41725792448`. Le compte réel n'a pas été créé : aucune connexion à une base de production n'est configurée dans cet environnement. Aucun identifiant ni mot de passe réel n'est inscrit dans le dépôt.

Politique de mot de passe : le minimum est fixé à 8 caractères dans le stockage, l'API et le contrat OpenAPI. Le compte réel reste à provisionner dans l'environnement central SEL ; aucun secret n'est inscrit dans ce rapport ni dans le dépôt.

Demande de seuil minimal de 8 caractères : test unitaire de frontière ajouté (8 accepté, 7 refusé) ; `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` et `git diff --check` passent localement. Les tests PostgreSQL restent à exécuter dans la CI pour le commit de cette mise à jour. Avant l'étape suivante, il restera à provisionner l'organisation centrale réelle et l'administrateur avec accès PostgreSQL protégé ; l'opérateur SEL sera défini par SEL, et les tests/runs CI du commit courant devront être verts.

### Transition vers la phase 2 — administration des tarifs (4 novembre 2026)

**Décision de progression :** la phase 1 n'est pas déclarée fermée. Le code de base des identités, sessions, rôles, grants et bootstrap initial est avancé et testé en CI, mais l'accès à la base centrale SEL et le provisionnement réel restent externes ; des opérations d'administration des utilisateurs/appareils/grants restent également à compléter. Conformément à la directive du propriétaire, le développement passe directement à la phase 2 sans présenter ce blocage externe comme une validation accomplie.

| Élément de phase 2 | Statut | Détails et preuve |
|---|---|---|
| Création de brouillons de tarifs | COMPLETED | Endpoint ADMIN versionné `/v1/admin/catalog/tariffs`, idempotence persistante, audit transactionnel ; les brouillons restent exclus du catalogue commercial. |
| Approbation commerciale | COMPLETED | Endpoint ADMIN exigeant une référence d'approbation fournie par SEL, enregistre l'approbateur et un événement d'audit ; aucune valeur réelle n'est créée par défaut. |
| Prévention de périodes approuvées qui se chevauchent | COMPLETED | Verrou consultatif PostgreSQL par service/catégorie et vérification transactionnelle ; une migration permet un état DRAFT sans référence et conserve une contrainte pour les versions approuvées. Concurrence vérifiée en CI PostgreSQL. |
| Contrat API | PASS local | OpenAPI expose les deux opérations, authentification, idempotency keys, champs et erreurs ; le test vérifie les routes et schémas. |
| Taxes, taux USD/CDF, catalogue solaire administrable, cache offline | NOT_STARTED dans cette tranche | À faire après validation PostgreSQL de la tranche tarifaire ; aucune fiscalité, parité ou valeur métier n'a été inventée. |

État de cette tranche : **code implémenté et validé, tests locaux exécutables passés, test PostgreSQL réussi en CI**. Le test `tariff_drafts_require_admin_approval_reference_and_non_overlapping_periods` couvre le chemin HTTP, le brouillon invisible au catalogue, l'approbation, la répétition concurrente d'une création avec la même clé, la réutilisation d'une clé sur une autre ressource, ainsi que deux approbations concurrentes de périodes qui se chevauchent (une seule doit être acceptée). Il reste ignoré localement faute de `TEST_DATABASE_URL` et de moteur Docker actif ; la migration `0007_tariff_drafts.sql` a été appliquée par le workflow PostgreSQL.

Validations locales du commit de travail, 4 novembre 2026 : `cargo fmt --all` (**PASS**), `cargo test --workspace` (**PASS pour tous les tests exécutables ; tests PostgreSQL ignorés selon leurs préconditions explicites**), `cargo clippy --workspace --all-targets -- -D warnings` (**PASS**), `cargo test -p sel-api --test postgres_catalog_api --no-run` (**PASS compilation du test PostgreSQL**), `cargo test -p sel-contracts --test openapi_contract` (**PASS**), `git diff --check` (**PASS**). Le test OpenAPI a d'abord échoué comme attendu car les endpoints n'étaient pas décrits, puis passe après documentation. Aucun tarif SEL n'a été approuvé, aucune donnée réelle modifiée et aucun déploiement réalisé.

Progression déclarée : Phase 1 **90 % — incomplète** (le pourcentage historique est un suivi de critères techniques, pas une disponibilité production) ; tranche « création + approbation tarifaire avec prévention de chevauchement » de la Phase 2 **100 %** ; Phase 2 globale **non chiffrée faute de lots entièrement découpés et pondérés**. La gestion complète des tarifs (notamment révocation/version suivante et projection offline) reste distincte. Prochaine tranche : administration versionnée du catalogue solaire, des taxes et des taux approuvés, puis cache offline, sans introduire de valeurs par défaut non validées.

Résultat CI pour le commit de code `138dd02f16408eed7fef4ed342f97d1a77ed7ab9` : **PASS**, backend Rust/PostgreSQL run `41734754734`, paquet serveur `41734754664`, build Windows Desktop `41734754663`, installateur candidat Windows `41734754655`. Les deux premières exécutions de la tranche avaient révélé des défauts de fixture uniquement : token de catalogue non authentifié (`7288ad6`) puis casse non normalisée dans l'assertion du test (`50f40da`). Elles ont été corrigées et la suite PostgreSQL complète est verte sur `138dd02`; aucun échec n'a été masqué ou désactivé. Le commit documentaire ultérieur peut encore déclencher de nouveaux workflows.

### Phase 2 — versions des référentiels et cache offline (travail en cours, 4 novembre 2026)

Cette mise à jour supersède l'état « NOT_STARTED » des taxes, devises, catalogue solaire et cache offline indiqué dans la transition initiale. Le travail ci-dessous est basé sur `26fd1d31c5b3e8060edb9c076a774fbd20de1057` et a été commité/poussé dans `2f4d58830ea4af89acb2d4dfd1deb408bba4020c`. Au moment de la rédaction, les workflows GitHub Actions correspondants sont **IN_PROGRESS**, sans conclusion vérifiée.

| Domaine / critère | Statut actuel | Code et preuve locale | Validation manquante |
|---|---|---|---|
| Cycle de vie des tarifs | COMPLETED (API/backend) | Versions listées, approuvées/révoquées avec audit, dates d'effet, remplacement historique et contrôle transactionnel des chevauchements. Tests PostgreSQL HTTP et CI PASS sur `c317117`. | L'administration visuelle sera livrée avec le frontend phase 7. |
| Catalogue solaire versionné | COMPLETED (API/backend) | Brouillons, approbation, révocation, dates d'effet et conservation du prix courant jusqu'à la veille d'une nouvelle version ; test PostgreSQL et CI PASS sur `c317117`. | L'administration visuelle sera livrée avec le frontend phase 7. |
| Profils fiscaux | COMPLETED (mécanisme) / BLOCKED_EXTERNAL (valeurs SEL) | Brouillon, approbation avec référence, remplacement daté, révocation auditée, RBAC et OpenAPI ; tests PostgreSQL réels PASS. Aucun profil fiscal SEL n'a été inventé. | SEL doit confirmer les profils, juridictions et assiettes. Les ambiguïtés restent bloquantes à l'émission. |
| Taux de change | COMPLETED (mécanisme) / BLOCKED_EXTERNAL (valeurs SEL) | Référentiels datés, source et approbation explicites ; brouillons exclus du catalogue ; tests PostgreSQL réels et CI PASS. Aucun taux n'est approuvé automatiquement. | SEL doit fournir/approuver les taux de production nécessaires. |
| Snapshot offline | COMPLETED (code/tests) | Contrat versionné, SHA-256 déterministe, projection SQLite transactionnelle des versions approuvées/révoquées, rejeu idempotent et révocations propagées ; tests SQLite, HTTP local et PostgreSQL CI PASS. | L'activation sur les postes réels dépend du provisionnement SEL et de la recette Windows. |
| Fichiers de facture côté serveur | PASS | Seules des données structurées sont stockées ; test de schéma et CI PASS, aucun PDF/fichier de facture côté serveur. | Aucune lacune identifiée dans le périmètre backend testé. |

**Progression Phase 2 — référentiels backend : 100 % des critères techniques.** Les API de versionnement/administration et le cache offline ont passé la CI avec PostgreSQL réel. Ce statut ne signifie ni que SEL a fourni/approuvé ses valeurs réelles, ni que les écrans d'administration du nouveau frontend sont terminés, ni que l'ensemble du produit est prêt pour la production.

Validations de l'arbre de travail : `cargo test --workspace` (**PASS**, aucun échec ; les tests PostgreSQL sont ignorés selon leur précondition explicite), `cargo test -p sel-storage --test catalog_snapshot` (**PASS**), `cargo test -p sel-local --test local_http catalog_refresh_fetches_and_persists_verified_snapshot_once` (**PASS**), `cargo test -p sel-api tax_and_exchange_rate_administration_is_admin_only` (**PASS**), `cargo test -p sel-contracts --test openapi_contract` (**PASS**), `cargo test -p sel-api --test postgres_catalog_api --no-run` (**PASS compilation uniquement**), `cargo clippy --workspace --all-targets -- -D warnings` (**PASS**), `cargo fmt --all` (**PASS**). Le test `catalog_snapshot_is_authenticated_atomic_and_idempotent` couvre le hash invalide, les répétitions, les révocations et le blocage d'un choix fiscal ambigu. `tax_and_fx_versions_require_approval_and_preserve_effective_history` compile mais reste **NOT_TESTED** sur PostgreSQL.

Blocage PostgreSQL local constaté lors de la rédaction initiale : aucune variable `TEST_DATABASE_URL`/`DATABASE_URL` n'est configurée et Docker Desktop n'était pas démarré. La CI a depuis appliqué puis validé les migrations et tests PostgreSQL réels sur son service dédié. Ce blocage reste une limite des essais locaux, pas de la CI distante.

### Phase 3 — clients et services (en cours, 4 novembre 2026)

Premier incrément : la recherche des abonnés est disponible dans les API centrale et locale, avec pagination et recherche insensible à la casse sur le code/nom. SQLite applique le filtrage Unicode en Rust pour que les noms accentués soient recherchés correctement (la fonction `lower()` native SQLite est limitée aux caractères ASCII). Le paramètre `q` est borné à 128 caractères et décrit dans OpenAPI.

Validations locales : `cargo test --workspace` (**PASS**, tests PostgreSQL ignorés faute de base), `cargo clippy --workspace --all-targets -- -D warnings` (**PASS**), `cargo fmt --all -- --check` (**PASS**), `git diff --check` (**PASS**), tests HTTP de recherche API et SQLite locale (**PASS**, y compris `Élodie` recherchée avec `élodie`), contrat OpenAPI (**PASS**). Les corrections de fixtures PostgreSQL de la Phase 2 sont incluses dans ce même arbre, mais attendent une nouvelle exécution CI.

**Progression Phase 3 : 15 % (estimation par critères de sortie)**. La recherche client est implémentée et testée localement ; le CRUD client, les relevés de compteurs, le parcours facture combinée de bout en bout et l'historique des factures restent à terminer. Le prochain lot porte sur les relevés : le modèle normatif prévoit compteurs rattachés à des comptes de service, index courant/précédent, période, quantité mesurée et corrections liées par `corrected_from_id`. PostgreSQL possède déjà les tables initiales, mais aucune route n'existe et SQLite ne possède pas encore leur projection. La conception devra préserver l'historique et rendre toute correction explicite ; aucune règle tarifaire ne sera ajoutée dans ce lot.

### Clôture des critères techniques de phase 1 — 4 novembre 2026

Cette section supersède les pourcentages historiques de phase 1 ci-dessus. **Phase 1 (contrats backend, autorisation et cycle de vie des identités) : critères de code 100 % réalisés** sur le jalon courant ; ce statut n'affirme ni un enrôlement sur un serveur réel, ni une recette Windows, ni une mise en production.

| Critère phase 1 | Statut | Preuve / limite |
|---|---|---|
| Sessions individuelles, expiration, rotation et révocation | COMPLETED côté code | Routes auth existantes ; changement d'accès révoque les sessions SQL dans la transaction. |
| Matrice de permissions par méthode et route, refus par défaut | COMPLETED côté code/test | Matrice unitaire étendue aux routes d'administration ; tous les rôles non ADMIN sont refusés pour les listes et mutations du répertoire. |
| Comptes ADMIN gérables sans suppression destructive | COMPLETED côté API | `GET/POST /v1/admin/users`, `PATCH /v1/admin/users/{id}` ; hash jamais retourné, compte désactivé par statut, doublon insensible à la casse protégé par verrou organisationnel, dernier ADMIN protégé, audit et révocation des sessions/grants. |
| Appareils consultables, enrôlables et révocables | COMPLETED côté API | Liste d'administration, enrôlement idempotent et révocation existants ; révoquer un appareil révoque aussi ses grants dans la même transaction. |
| Grants offline consultables et révocables | COMPLETED côté API | Liste sans signature, révocation persistée (`revoked_at`) et audit ; la validation de sync exclut les grants révoqués ; répétition de révocation sans effet supplémentaire. Migration additive `0008_admin_directory.sql`. |
| Amorçage premier administrateur | COMPLETED côté code/CI antérieure | `sel-cli bootstrap-admin`, transaction à usage unique, entrée auditée ; l'organisation doit déjà être provisionnée. |
| Contrats OpenAPI des routes d'administration | COMPLETED côté contrat | `openapi_contract` vérifie YAML, opérations ajoutées et sécurité Bearer. |
| Identité et sécurité réelles de SEL | BLOCKED_EXTERNAL | Aucun accès de production fourni/utilisé pour provisionner l'organisation, l'administrateur, les clés de production ou les appareils. Aucun compte réel ni secret n'a été créé. |
| Interface Windows d'administration | NOT_IN_PHASE_1 | Les endpoints sont prêts côté backend ; les écrans relèvent de la phase frontend dédiée et ne sont pas prétendus utilisables dans le client V2 à ce stade. |

Fichiers de ce jalon : `v2/crates/sel-storage/src/admin.rs`, `v2/crates/sel-storage/src/postgres.rs`, `v2/crates/sel-api/src/lib.rs`, `v2/migrations/postgres/0008_admin_directory.sql`, `v2/openapi/sel-v2.yaml`, tests API/OpenAPI et ce rapport. Le test PostgreSQL `admin_directory_creates_lists_and_suspends_users_without_destructive_delete` couvre création, doublon casse-insensible, séparation inter-organisation, refus RBAC, absence du hash, suspension, révocation de session et prévention de l'auto-désactivation. Le test device/grants couvre listes, révocation répétée et persistance de `revoked_at`.

Validations locales de ce jalon : `cargo fmt --all -- --check` (**PASS**), `cargo clippy --workspace --all-targets -- -D warnings` (**PASS**), `cargo test --workspace` (**PASS**, tous les tests actifs réussis ; les tests PostgreSQL sont ignorés localement selon leurs préconditions), `cargo test -p sel-api --test postgres_admin_directory --no-run` (**PASS compilation**), `cargo test -p sel-contracts --test openapi_contract` (**PASS**), test RBAC ciblé (**PASS**). Le moteur PostgreSQL local n'étant pas configuré, le parcours d'administration, la révocation des grants et l'application de migration ont été exécutés en CI PostgreSQL et sont PASS sur le commit de code `3d30e661f9aec647ca940f48843b1184c035601b`.

CI GitHub sur le commit de code `3d30e661f9aec647ca940f48843b1184c035601b` : **4/4 PASS** — backend et tests PostgreSQL `41740280531`, paquet serveur `41740280600`, build Windows Desktop `41740280370`, candidat Windows x64 `41740280366`. Le candidat MSI/EXE a été téléversé comme artefact d'acceptation de workflow ; aucune release n'a été publiée. Authenticode et publication ont été sautés par les conditions normales de la PR. Aucun merge dans `main`, déploiement, accès à la base réelle SEL ou modification de données réelles n'a eu lieu.

### Mise à jour de progression — 4 novembre 2026 (travail en cours)

À partir du commit distant `27b67b7ec1cfcc87a81892037ada6c0b51331384`, l'inspection des diagnostics CI a confirmé un échec du test `two_sqlite_devices_converge_through_real_postgres_and_http` : des références fiscales globales et conservées par les fixtures faisaient sélectionner plusieurs profils pour la même période. La fixture de convergence est isolée sur une période historique (`2000-11`), avec tarifs et taxes à effet en `2000-01-01`, au lieu de dépendre de l'état laissé par d'autres tests. Les deux insertions du catalogue solaire utilisent aussi désormais la clé unique versionnée `(sku, effective_from)`. Ces corrections sont locales et restent à confirmer par le nouveau job PostgreSQL CI.

Le même incrément ajoute `GET /v1/invoices` côté API centrale avec filtres par abonné et période, limite bornée à 100, isolation organisationnelle dans la requête SQL, statut et date d'émission issus de la ligne centrale et contenu provenant du snapshot structuré. La route est inscrite dans le contrôle RBAC et OpenAPI ; le test PostgreSQL commercial vérifie les filtres et la cohérence de l'identifiant retourné. Le mode mémoire ne masque plus une erreur de sérialisation en renvoyant un objet vide.

Validation fraîche de l'arbre de travail : `cargo fmt --all -- --check` (**PASS**), `cargo test --workspace` (**PASS pour tous les tests exécutables**), `cargo clippy --workspace --all-targets -- -D warnings` (**PASS**), `git diff --check` (**PASS**). Les tests d'intégration PostgreSQL, dont la convergence et l'historique central, sont ignorés localement car `TEST_DATABASE_URL` et un serveur PostgreSQL de test ne sont pas configurés ; la compilation locale ne vaut pas validation d'exécution PostgreSQL. Le dernier commit distant testé, `27b67b7`, a trois contrôles verts (`test-build`, `validate-package`, `build-candidate`) et le contrôle `rust` rouge sur l'ambiguïté de fixture décrite plus haut. La correction locale n'a pas encore de SHA distant ni de résultat CI.

**État de progression remplacé par la mise à jour ci-dessous : Phase 2 backend 100 % ; Phase 3 15 %.** Aucun tarif réel n'a été créé ni approuvé ; aucun poste SEL ni serveur réel n'a été modifié et aucun déploiement n'a eu lieu.

Le workflow Rust du SHA `025729158fa23c4842edc8039f509390190e1485` a validé `fmt`, `clippy` et `cargo test --workspace`, puis échoué dans le test de convergence PostgreSQL sur une erreur de syntaxe de fixture : la seconde ligne d'insertion des tarifs omettait `approval_ref`, produisant des listes `VALUES` de longueurs différentes. La fixture est corrigée en renseignant la même référence de test que pour l'électricité. `cargo fmt --all -- --check`, la compilation ciblée `cargo test -p sel-local --test two_device_convergence --no-run` et `git diff --check` passent sur la correction locale. Le scénario réel PostgreSQL doit être réexécuté par la CI du prochain SHA avant de valider la convergence.

La CI du SHA `dfccb8a4a52d84c4a8ad1dead26abb7aba27ea0e` a ensuite confirmé que l'erreur SQL était résolue, mais a reproduit l'ambiguïté fiscale : `CentralStore::load_tariffs` sélectionne les profils par service et période et ne les filtre pas par organisation ; le test PostgreSQL `postgres_void` laissait également un profil électricité applicable à la période fixe 2000. Les tests du workflow partagent une base, donc des organisations distinctes n'isolaient pas ces profils globaux. Le test de convergence choisit maintenant une année de facturation aléatoire éloignée des autres fixtures et utilise la même date d'effet dans ses tarifs/taxes centraux et SQLite. La compilation ciblée et `fmt --check` passent ; l'exécution PostgreSQL de cette variante reste à confirmer par CI. Cette modification ne change pas le moteur de sélection financière ni les règles applicables en production.

Mise à jour après exécution du SHA `6d603142f8d44f0eb29340308ad0d2036124ed44` : le test PostgreSQL a démontré que la seule variation de période ne suffisait pas, car les profils fiscaux à validité ouverte d'autres tests continuaient à s'appliquer. Le problème est le partage de tables de référence globales entre tests, pas le comportement métier. Le test `two_device_convergence` crée désormais un schéma PostgreSQL isolé, y exécute les migrations et les véritables appels HTTP/synchronisation, puis supprime ce schéma après le scénario. La branche de test reste fondée sur les profils approuvés qu'elle sème elle-même ; aucun contrôle fiscal n'est neutralisé. `fmt --check`, `cargo test -p sel-local --test two_device_convergence --no-run` et `git diff --check` passent localement. Le workflow Rust sur `6d60314` échoue encore au test de convergence, avant cette isolation; la nouvelle version doit passer la CI PostgreSQL pour que ce parcours soit déclaré validé.

Sur le SHA `e7002001411c74e4d7cfdf1b50be5694b1c7e8ea`, le parcours multi-postes isolé a réellement réussi dans PostgreSQL et via HTTP (`two_sqlite_devices_converge_through_real_postgres_and_http ... ok`). La suite a ensuite échoué sur `utility_invoice_sync_persists_two_lines_including_zero_water`, à cause de références fiscales de tests précédents conservées dans le schéma PostgreSQL partagé. La fixture `postgres_void` est bornée au `2000-12-31`; le test de facture synchronisée, qui a lui-même besoin de créer des profils fiscaux, utilise désormais son propre schéma jetable. Ces changements empêchent la pollution inter-tests sans modifier le moteur commercial et attendent la CI du prochain commit.

### Clôture de la phase 2 backend — 4 novembre 2026

Le commit de code `c317117f2cdabaef16b8681f3ed7669866aa0a6a` a **4/4 workflows PASS** : Rust/PostgreSQL run `41775650112`, package serveur `41775650295`, build Windows Desktop `41775650088`, candidat Windows x64 `41775650078`. Le workflow Rust a passé `fmt`, Clippy, `cargo test --workspace` et les tests PostgreSQL d'intégration. En particulier, `two_sqlite_devices_converge_through_real_postgres_and_http` et `utility_invoice_sync_persists_two_lines_including_zero_water` passent dans leurs schémas isolés. L'EXE/MSI/bootstrapper ont été compilés et téléversés comme candidat d'acceptation non publié.

**Phase 2 — Référentiels backend : 100 % des critères techniques. Phase 3 — Clients et services : 15 %.** La phase 2 est fermée pour le backend : tarifs, taxes, devises, catalogue et cache offline sont versionnés, modifiables par API, contrôlés par rôle et validés par les tests/CI. Les tarifs, profils fiscaux et taux réels demeurent `BLOCKED_EXTERNAL` tant que SEL ne les a pas confirmés. Leur administration visuelle appartient au frontend phase 7. Cette clôture ne certifie ni une recette Windows sur poste SEL ni une mise en production. Branche `monarque/sel-v2-backend`, commit `c317117`; worktree vérifié propre avant ce changement documentaire. Aucun merge, release ou déploiement.

### Phase 3 — premier incrément des relevés locaux (4 novembre 2026)

**Progression estimée de phase 3 : 22 %.** La migration SQLite additive `0010_meter_readings.sql` et les routes HTTP locales permettent maintenant de créer un compte de service avec un état de raccordement explicite, rattacher un compteur dont l'unité doit correspondre au service, saisir/lire des index datés et corriger un relevé par ajout d'une nouvelle ligne liée à l'original. Rust calcule `current - previous`; les doublons d'un relevé initial pour le même compteur/période sont refusés. Le test local exerce aussi un relevé à zéro et la persistance après fermeture/réouverture SQLite.

Fichiers modifiés : `v2/crates/sel-contracts/src/lib.rs`, `v2/crates/sel-storage/src/sqlite.rs`, `v2/crates/sel-storage/src/lib.rs`, `v2/crates/sel-local/src/lib.rs`, `v2/crates/sel-local/tests/local_http.rs`, `v2/migrations/sqlite/0010_meter_readings.sql`.

Validations exécutées localement après ces changements : `cargo fmt --all -- --check` **PASS**, `cargo clippy --workspace --all-targets -- -D warnings` **PASS**, `cargo test --workspace` **PASS pour les tests exécutables localement** ; les tests intégration PostgreSQL marqués `ignored` restent non exécutés, faute de `TEST_DATABASE_URL`. Le test HTTP `local_meter_readings_are_durable_validated_and_corrected_append_only` passe, après avoir vérifié l'échec initial `404` avant l'implémentation.

Limites de ce jalon : les nouvelles entités de compteurs/relevés ne sont pas encore exposées dans l'API centrale, la migration PostgreSQL correspondante n'est pas créée, aucun événement de relevé n'est encore envoyé dans l'outbox/synchronisé, les relevés ne sont pas encore la source des quantités de facturation, et aucun écran de saisie n'est raccordé. **Ce sous-lot est partiel et n'est ni commité ni poussé ; aucun parcours central ou multi-postes n'est revendiqué comme validé.** Prochain sous-lot : contrat et repository PostgreSQL transactionnel, migration additive centrale et tests HTTP PostgreSQL isolés ; ensuite seulement, commande/outbox et projection SQLite à la synchronisation.

### Phase 3 — comptes de service, compteurs et synchronisation (4 novembre 2026)

Cette mise à jour supersède les limitations du jalon local précédent. Les migrations additives SQLite `0010_meter_readings.sql` et PostgreSQL `0012_meter_readings.sql` sont désormais appliquées par les tests. Les routes locales et centrales couvrent les comptes de service, les compteurs, les relevés et les corrections. La saisie locale inscrit l'entité métier, le journal et la commande d'outbox dans la même transaction SQLite. Les nouvelles commandes (`SERVICE_ACCOUNT_CREATED`, `METER_CREATED`, `METER_READING_CREATED`, `METER_READING_CORRECTED`) sont acceptées par PostgreSQL selon les permissions de l'acteur et les autorisations d'appareil.

| Critère | Statut | Preuve / limite |
|---|---|---|
| Comptes de service, compteurs et index | COMPLETED côté API/backend | Isolation par organisation, unité déterminée par service, unicité compte/numéro et relevé initial/période, verrouillage du compteur, index monotones et quantité calculée en Rust. |
| Corrections d'index | COMPLETED côté code/tests | Ajout append-only lié au relevé précédent et motif obligatoire ; corrections interdites sur une ligne déjà corrigée et validation de la période suivante. |
| Outbox offline des relevés | COMPLETED côté stockage | Les quatre opérations sont écrites atomiquement avec leur événement. Le test HTTP local vérifie le journal, l'outbox et la persistance après réouverture. |
| Application centrale + publication | COMPLETED côté repository | Transaction PostgreSQL comprenant l'effet métier, l'accusé d'inbox et l'événement central ; déduplication par événement, validation des dépendances et des conflits. |
| Projections SQLite | COMPLETED côté stockage/tests | Comptes, compteurs et historique de relevés appliqués de manière idempotente ; les types inconnus restent auditables et ne sont pas silencieusement perdus. |
| Régression API, SQLite et PostgreSQL | PASS local | Sur une base de test PostgreSQL fraîche : `cargo test -p sel-api -p sel-storage -p sel-local --tests -- --include-ignored` — **81 réussis, 0 échec**. Après extension du test de convergence pour les relevés, `cargo test -p sel-local --test two_device_convergence two_sqlite_devices_converge_through_real_postgres_and_http -- --exact --ignored` réussit également. |
| Formatage et analyse statique | PASS local | `cargo fmt --all -- --check` et `cargo clippy --workspace --all-targets -- -D warnings` réussis après les modifications. |
| Facture calculée à partir des index validés | NOT_STARTED | Les quantités doivent encore être reliées à un relevé accepté, avec garde contre la double facturation et historique cohérent ; l'interface ne doit pas recalculer de montants. |
| Client Windows V2 utilisable pour ce parcours | NOT_STARTED | Aucun écran de relevés ou flux d'impression ne fait partie de ce lot. Aucun poste SEL ni imprimante n'a été testé. |

La première matrice exécutée sur une base de test déjà utilisée a rencontré une fixture de catalogue non isolée (6 produits présents au lieu d'1). Le test et la matrice entière passent sur une base fraîche ; aucun test n'a été désactivé et aucun contrôle métier n'a été réduit.

**Progression Phase 3 : 45 % — estimation des critères de la phase, non de la disponibilité du produit.** Le test réel `two_sqlite_devices_converge_through_real_postgres_and_http` couvre maintenant création du client, facture combinée, paiement partiel, compte de service, compteur, relevé à zéro et correction append-only ; l'outbox passe par le serveur HTTP PostgreSQL, puis le second SQLite reçoit le même historique et les mêmes identifiants. Les ventes solaires et paiements concurrents restent aussi couverts dans ce scénario. Restent l'émission alimentée par les index, le CRUD et l'historique client/facture, les autres opérations commerciales à compléter et les parcours correspondants du nouveau frontend Windows. La disponibilité SEL et la recette sur poste réel restent non vérifiées.

État Git à la clôture du lot : commit `2e0cf0bb70e75bc2b913797c354b7b6674999611`, poussé sur `monarque/sel-v2-backend` ; la CI associée a été déclenchée et reste à confirmer. Aucun merge dans `main`, release, déploiement ou accès aux données SEL n'a eu lieu. Les validations Windows sur poste SEL et l'approbation métier des tarifs réels restent externes.

### Phase 3 — facturation liée aux relevés et convergence (4 novembre 2026)

Cette entrée remplace l'état précédent « facture calculée à partir des index : NOT_STARTED ». Le contrat `UtilityBillingRequest` accepte maintenant, de manière rétrocompatible, les identifiants des relevés d'électricité et d'eau. Lorsque ces identifiants sont fournis, les quantités soumises par l'interface ne sont pas la source de calcul : le backend résout les relevés courants du client, du service, de l'unité et de la période, puis reprend leurs quantités calculées par Rust. Les factures historiques et les requêtes V2 existantes sans références de relevés restent lisibles et compatibles.

Les migrations additives `0011_invoice_meter_readings.sql` (SQLite) et `0013_invoice_meter_readings.sql` (PostgreSQL) enregistrent la provenance des deux lignes de service. La transaction d'émission revérifie les relevés au moment de la validation, compare les quantités des lignes financières aux quantités des index, puis lie les deux relevés à la facture. Une contrainte unique interdit de réutiliser le même relevé pour une autre facture. Les corrections append-only d'un relevé déjà facturé sont rejetées localement, par l'API centrale et lors de la réception d'une correction synchronisée. La projection de la facture vers un autre poste vérifie les références, les quantités et l'absence de conflit avant de conserver les liens, de façon idempotente.

| Critère | Statut | Preuve |
|---|---|---|
| Contrat versionné et rétrocompatible pour rattacher deux relevés | COMPLETED côté code | `UtilityBillingRequest.meter_readings` facultatif ; les anciennes requêtes sans champ restent valides. |
| Calcul à partir des index et rejet des quantités divergentes | COMPLETED côté code/test | Le test HTTP local crée 50 kWh et 2,5 m³, soumet volontairement 999/999 puis vérifie que la facture conserve 50/2,5. |
| Traçabilité et unicité de l'utilisation d'un relevé | COMPLETED côté SQLite/PostgreSQL | Migrations additives, liaison facture/service, contrainte unique sur l'identifiant du relevé ; insertion et validation dans la transaction financière. |
| Protection de l'historique après émission | COMPLETED côté code/tests | Le test local refuse la correction d'un index facturé ; le scénario PostgreSQL multi-postes vérifie aussi `ReadingAlreadyInvoiced`. |
| Projection de la facture et de sa provenance sur le second poste | COMPLETED côté code/test d'intégration | `two_sqlite_devices_converge_through_real_postgres_and_http` vérifie les deux identifiants, les lignes, le total et le solde dans deux SQLite et PostgreSQL. |
| Tests workspace et PostgreSQL | PASS local | `cargo test --workspace -- --include-ignored` sur PostgreSQL 16.4 jetable : tous les tests activés réussissent, aucun échec ; le test migration utilise un pont temporaire vers `pg_dump` dans le conteneur. |
| Formatage et analyse statique | PASS local | `cargo fmt --all -- --check` et `cargo clippy --workspace --all-targets -- -D warnings` réussissent après le dernier changement. |
| CI GitHub sur le commit `71d24f86ddeb7c7cc8340495d5f44df39cea4765` | PASS | Backend : [run 41786475599](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41786475599) ; candidat Windows : [run 41786475576](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41786475576) ; build Windows Desktop : [run 41786475587](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41786475587) ; paquet serveur : [run 41786475558](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41786475558). Les quatre workflows sont terminés avec succès sur ce SHA. La signature Authenticode et la publication sont restées ignorées. |
| Interface Windows pour choisir/revoir les index | NOT_STARTED | Le formulaire du nouveau client n'a pas encore été raccordé à ce contrat ; aucune interface n'est déclarée utilisable pour ce flux. |

Le premier test HTTP a été exécuté avant implémentation et a échoué sur la quantité facturée `999` au lieu de `50`, ce qui reproduisait le défaut. Après correction, le test local passe. Le parcours PostgreSQL a d'abord signalé une différence de représentation décimale (`0.095`/`0.095000`) ; les assertions comparent désormais les valeurs `Decimal`, sans tolérance ni arrondi additionnel. L'exécution suivante du scénario multi-postes a réussi.

**Progression estimée de phase 3 : 65 %.** La facturation alimentée par des relevés est maintenant démontrée dans le parcours offline → PostgreSQL → second poste, et les quatre workflows GitHub du commit `71d24f86ddeb7c7cc8340495d5f44df39cea4765` sont `PASS` (preuves et limites ci-dessus). Il reste notamment les parcours CRUD/historique client et facture, l'intégration de l'interface Windows V2, l'administration opérationnelle complète et les recettes sur poste SEL. Phase 2 (référentiels backend) reste à 100 % de ses critères techniques ; la validation par SEL des valeurs tarifaires et fiscales réelles demeure `BLOCKED_EXTERNAL`.

Les tests ont utilisé exclusivement une instance PostgreSQL jetable et des SQLite créées par les tests. Le conteneur de test est supprimé après vérification. Aucun compte, tarif, facture ni fichier de SEL réel n'a été consulté ou modifié. Aucun merge, release ou déploiement n'a été effectué.

### Phase 3 — filtres d'historique des factures local et central (4 novembre 2026)

Les listes `GET /local/v1/invoices` et `GET /v1/invoices` acceptent maintenant le même contrat de recherche par `customer_id`, période `YYYY-MM`, état du cycle de vie et limite bornée. Le filtrage s'effectue dans la requête SQLite/PostgreSQL, avec contrôle du périmètre d'organisation ; l'API locale ne renvoie plus l'ensemble des factures quand un client ou une période est sélectionné. Les statuts inconnus et périodes invalides sont rejetés par HTTP 422. Le contrat OpenAPI central précise l'énumération de statuts. Les factures restent immuables : ce lot ne modifie ni documents, ni snapshots, ni lignes ni soldes.

| Critère | Statut | Preuve |
|---|---|---|
| Filtre local client + période + état | PASS | Le test HTTP local couvre une facture `LOCAL_PENDING` et une facture `VOID`, vérifie chacune est isolée par son filtre et qu'un état inconnu reçoit HTTP 422. |
| Filtre PostgreSQL après annulation | PASS | `invoice_void_is_audited_and_idempotent` exécuté sur PostgreSQL 16.4 jetable ; après annulation, la requête filtrée retourne une seule facture `VOID` et un état inconnu reçoit HTTP 422. |
| Contrat HTTP et documentation | PASS local | `cargo test -p sel-contracts --test openapi_contract` ; le paramètre `status` et ses états sont documentés. |
| Régression workspace | PASS local | `cargo test --workspace` : tous les tests exécutés passent. Les tests marqués `ignored` qui exigent `TEST_DATABASE_URL` ne sont pas tous inclus dans cette commande ; le test PostgreSQL de ce lot a été lancé séparément. |
| Analyse statique et format | PASS local | `cargo clippy --workspace --all-targets -- -D warnings` et `cargo fmt --all -- --check`. |
| CI GitHub du commit fonctionnel `a21a2846f92d3730a73ee9e6ffa8a0cbfca2062a` | PASS | Backend run `41788841797`, build Windows Desktop `41788841693`, paquet serveur `41788841800`, candidat Windows `41788841648` ; les quatre workflows ont réussi sur ce commit. La signature Authenticode et la publication sont restées ignorées. |

Le test HTTP local a d'abord échoué comme attendu : il renvoyait 2 factures pour une requête filtrée qui devait en retourner 1. Après implémentation, le test passe. Une base PostgreSQL 16.4 isolée a ensuite confirmé le filtre central ; le conteneur a été arrêté, sans accéder aux données réelles de SEL.

**Progression estimée : Phase 2 100 % ; Phase 3 68 %.** La prochaine tranche Phase 3 vise la cohérence du relevé client et de l'historique complet des factures côté local/central, puis les lacunes restantes de création, modification et archivage client. L'interface Windows V2, l'administration visuelle, la recette sur le poste SEL et l'approbation métier des paramètres réels restent à réaliser ou confirmer.

### Phase 3 — PATCH client à champs tri-état et compatibilité de synchronisation (4 novembre 2026)

Le contrat de modification d'un client distingue désormais un champ omis (conserver sa valeur), un champ explicitement `null` (effacer la valeur) et une valeur fournie (remplacer). Cette distinction s'applique aux coordonnées adresse/téléphone, dans l'API locale et centrale, ainsi que dans les projections SQLite/PostgreSQL. Les nouvelles commandes locales portent `patch_format_version: 2`. Pour préserver les anciens clients qui sérialisent parfois une valeur absente en `null`, les événements non versionnés gardent la valeur existante ; seuls les payloads explicitement versionnés en V2 peuvent demander un effacement.

Les routes locale et centrale rejettent également un nom légal vide et tout statut autre que `ACTIVE` ou `ARCHIVED`. Les mises à jour conservent la vérification de révision/`If-Match`; les données financières et factures historiques ne sont pas concernées.

| Critère | Statut | Preuve |
|---|---|---|
| Tri-état du contrat PATCH | PASS local | `PatchValue<T>` et test HTTP local : champ omis conservé, `null` explicite effacé, valeur définie remplacée. |
| Événement local et outbox | PASS local | Le stockage SQLite écrit le marqueur de format 2 dans le payload de synchronisation dans la même transaction que la modification. |
| Projection SQLite rétrocompatible | PASS local | Test de pull : ancien payload non versionné avec `phone:null` conserve le téléphone ; payload versionné V2 l'efface, sans perdre l'adresse omise. |
| API PostgreSQL réelle | PASS local | `postgres_customer_api` exécuté sur PostgreSQL 16.4 jetable ; le PATCH `null` persiste et les champs non fournis sont préservés après nouvelle instance de l'application. |
| Synchronisation PostgreSQL réelle | PASS local | `postgres_sync` exécuté sur PostgreSQL 16.4 jetable ; ancien événement non versionné préserve le téléphone, événement V2 l'efface, les accusés sont acceptés et le pull contient les trois changements. |
| Régression workspace | PASS local | `cargo test --workspace` réussit. Les tests d'intégration qui requièrent PostgreSQL sont ignorés par défaut ; les deux tests PostgreSQL affectés ont été lancés séparément et réussissent. Les autres tests PostgreSQL ignorés ne sont pas revendiqués comme exécutés dans cette tranche. |
| Formatage / Clippy | PASS local | `cargo fmt --all -- --check` et `cargo clippy --workspace --all-targets -- -D warnings` réussissent. `git diff --check` passe. |
| CI GitHub du commit `34f7665ba0d1f80537da292f6b7e2ed57f7e7566` | PASS | Backend avec tests PostgreSQL : [run 41792233131](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41792233131) ; paquet serveur : [run 41792233159](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41792233159) ; build desktop Windows : [run 41792233132](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41792233132) ; candidat Windows : [run 41792233145](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41792233145). Les quatre workflows sont terminés avec succès sur ce SHA. |

Le test local a été d'abord exécuté en rouge pour reproduire le défaut : un PATCH contenant `phone:null` ne supprimait pas le téléphone. Après correction et ajout de la version de payload, les tests SQLite et PostgreSQL ci-dessus passent. L'image PostgreSQL utilisée est `postgres:16`, dans un conteneur temporaire `sel-phase3-customer-pg` sur le port local 55432 ; aucune donnée SEL réelle n'a été utilisée. Le conteneur doit être arrêté après la validation.

**Progression estimée : Phase 2 100 % ; Phase 3 71 %.** Le code, les tests locaux, les deux parcours PostgreSQL touchés et les quatre workflows GitHub du commit `34f7665ba0d1f80537da292f6b7e2ed57f7e7566` sont validés. Le CRUD client est encore partiel : recherche, création, lecture et modification sont disponibles côté APIs, mais l'expérience complète dans le nouveau client Windows, les opérations restantes et la recette sur poste SEL ne sont pas validées. Ce pourcentage est un suivi des critères de Phase 3, pas une déclaration de disponibilité opérationnelle.

### Phase 3 — données de document disponibles hors connexion (4 novembre 2026)

L'agent local expose désormais `GET /local/v1/invoices/{id}/document-data`, en parité avec l'enveloppe centrale. Il retourne le snapshot métier conservé dans SQLite, le schéma de document versionné, le modèle `v2-a4-1`, une référence QR interne `SEL-V2-{invoice_id}` et l'origine `LOCAL_PENDING` tant que l'opération n'est pas acceptée centralement (puis `CENTRAL_ACCEPTED`). La route ne génère ni ne stocke de PDF et ne prétend à aucune authentification fiscale. Elle permet au client de préparer le document sans Internet à partir de la même empreinte financière immuable.

| Critère | Statut | Preuve |
|---|---|---|
| Route locale de document-data | PASS local | Le test HTTP émet une facture eau + électricité fondée sur les relevés, puis lit l'enveloppe depuis SQLite. |
| Cohérence du snapshot | PASS local | L'empreinte `snapshot_sha256` retournée par l'enveloppe est égale à celle de la facture persistée. |
| Référence et état d'origine | PASS local | Test vérifie schéma 1, modèle `v2-a4-1`, référence QR interne stable et `LOCAL_PENDING`. |
| Régression de l'agent local | PASS local | `cargo test -p sel-local --test local_http` : 7 tests réussis ; `cargo test --workspace` passe, les tests qui exigent PostgreSQL sont ignorés par défaut. |
| Format et Clippy | PASS local | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` et `git diff --check`. |
| CI GitHub du commit `dd3da811ea72a5961edac46419cf0a88392fbe4e` | PASS | Backend avec tests PostgreSQL : [run 41793391038](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41793391038) ; build Windows Desktop : [run 41793390899](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41793390899) ; paquet serveur : [run 41793390885](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41793390885) ; candidat Windows : [run 41793390992](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41793390992). Les quatre workflows sont terminés avec succès. Les paquets sont des candidats d'acceptation non signés/non publiés, pas une livraison SEL. |
| Impression réelle et PDF client | NOT_TESTED | Aucun nouveau frontend/générateur PDF n'est raccordé dans ce lot ; aucune imprimante n'a été utilisée. |

Le test HTTP a d'abord échoué parce que l'endpoint local n'existait pas ; après ajout, il a réussi. Une première exécution intermédiaire du test utilisait une URL mal formée construite à partir de la valeur JSON avec guillemets ; le test a été corrigé pour extraire l'identifiant en chaîne et passe maintenant. Aucun PDF, base réelle ni donnée SEL n'a été créé ou modifié.

**Progression déclarée : Phase 2 100 % ; Phase 3 74 %.** Les tests locaux et les quatre workflows du commit `dd3da811ea72a5961edac46419cf0a88392fbe4e` réussissent. Le client Windows pourra consommer l'enveloppe locale, mais la définition de toutes les données administratives nécessaires au document, la génération PDF et l'impression doivent encore être implémentées et vérifiées dans la phase frontend dédiée.

### Phase 3 — conservation de la période et de l’échéance de facture (4 novembre 2026)

Le contrat de réponse `UtilityBillingResponse` inclut maintenant la période facturée (`YYYY-MM`) et la date limite de paiement. Ces métadonnées accompagnent l’aperçu, l’émission, le snapshot documentaire, la liste d’historique, le document JSON local et les projections de synchronisation. SQLite et PostgreSQL les vérifient contre la requête de facturation avant d’accepter une nouvelle facture. Lors de la confirmation d’une facture locale, la projection vérifie ces valeurs, garde les montants/lignes/empreinte inchangés et complète uniquement les métadonnées manquantes d’un ancien snapshot à partir de sa période en base et de la commande source. Une échéance historique inconnue demeure inconnue ; aucune date n’est inférée.

Les anciens snapshots restent désérialisables grâce aux valeurs par défaut des nouveaux champs. Pour ceux-ci, la période est relue de la colonne `invoices.period` ; une date d’échéance absente du snapshot ancien n’est jamais fabriquée. L’outil de conversion V1 conserve une échéance seulement si le manifeste source en fournit une. Le schéma OpenAPI décrit les deux champs. Aucune migration de tables n’est nécessaire.

| Critère | Statut | Preuve |
|---|---|---|
| Reproduction HTTP du défaut local | PASS | `utility_invoice_uses_and_binds_the_selected_meter_readings` vérifie qu’une échéance fournie apparaît dans la réponse et dans `/document-data` SQLite. Le test ciblé passe. |
| Persistance centrale après reconnexion du store | PASS | `utility_issue_is_atomic_idempotent_and_survives_restart` exécuté, sans `ignore`, contre PostgreSQL 16 jetable ; vérifie la réponse, l’historique, l’idempotence et la relecture après recréation du store, échéance `2026-12-15` comprise. |
| Convergence offline → PostgreSQL → second SQLite | PASS | `two_sqlite_devices_converge_through_real_postgres_and_http` exécuté contre le même PostgreSQL jetable ; les périodes et dates d’échéance sont comparées entre la facture initiale et les réponses des deux postes et du serveur. |
| Suite workspace | PASS | `cargo test --workspace` : tous les tests actifs passent. Les tests PostgreSQL non inclus par défaut restent explicitement signalés `ignored` ; les deux parcours critiques ci-dessus ont été exécutés séparément sur une base réelle jetable. |
| Format et analyse statique | PASS | `cargo fmt --all -- --check` et `cargo clippy --workspace --all-targets -- -D warnings` passent après le changement. |
| OpenAPI et compatibilité des snapshots historiques | PASS code/tests | Le contrat OpenAPI valide ; `billing_period` et `due_date` ont des valeurs Serde par défaut. Le chemin PostgreSQL complète la période via la colonne existante et laisse la date inconnue à `null`. |
| CI GitHub sur le commit `17d3f9b62259ef36f1179ac35017af8be0f8d8a1` | PASS | Backend : [run 41796163039](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41796163039) ; build Windows : [run 41796163072](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41796163072) ; paquet serveur : [run 41796163149](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41796163149) ; candidat Windows : [run 41796163100](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41796163100). Les quatre workflows sont terminés `success` sur ce SHA. |
| PDF/impression et recette du client Windows | NOT_TESTED | Hors de ce correctif backend ; aucune impression, recette matérielle SEL ni validation de l’interface Windows réalisée. |

Les tests ont utilisé uniquement PostgreSQL `postgres:16-alpine` jetable (`sel-phase3-due-date-pg`, port local 55432) et des bases SQLite éphémères. Aucune donnée SEL réelle n’a été lue ou modifiée. Conteneur arrêté et supprimé après exécution.

**Progression estimée : Phase 2 100 % ; Phase 3 75 %.** Ce point de phase 3 progresse d’un pour cent car le parcours de facturation conserve désormais période et échéance sur le poste local, le serveur PostgreSQL et un second poste, après reconnexion et redémarrage du store central. Les parcours UI du nouveau client, la génération/impression PDF, les workflows d’administration et la recette sur poste SEL restent à faire ou à vérifier. Ce pourcentage mesure les critères de développement de phase, pas l’aptitude à exploiter ou livrer le système.

### Phase 3 — modification versionnée des points de service (4 novembre 2026)

Le point de service peut désormais être consulté et modifié dans les API locale et centrale. Les champs modifiables sont la référence de contrat, l’état de raccordement et la date d’activation ; l’identité du client, le service et l’historique des compteurs restent immuables. Chaque modification exige `If-Match`, incrémente une révision et rejette une modification vide ou périmée. Les données SQLite sont écrites dans la même transaction que la commande et l’outbox. PostgreSQL applique les mises à jour synchronisées sous verrou de ligne, vérifie la révision attendue, puis enregistre la nouvelle projection et l’accusé dans une transaction ; les changements centraux alimentent les autres postes.

| Critère | Statut | Preuve |
|---|---|---|
| Révisions ajoutées sans migration destructive | PASS | Migrations `sqlite/0012_service_account_revisions.sql` et `postgres/0014_service_account_revisions.sql`, avec valeur initiale 1 pour les lignes existantes ; les bases d’essai ont migré avec succès. |
| Modification locale hors ligne et concurrence optimiste | PASS | `service_account_contract_can_be_updated_with_revision_control_offline` : contrat/raccordement modifiés, révision 2, rejeu périmé HTTP 409, PATCH vide HTTP 422, un seul événement outbox. Test exécuté dans `cargo test --workspace`. |
| Projection sur un second SQLite | PASS | `pulled_service_account_revision_updates_existing_local_projection_idempotently` met à jour une projection existante, rejoue le changement et conserve une seule ligne à la révision 2. |
| Synchronisation réelle SQLite → PostgreSQL | PASS | `offline_meter_account_and_reading_events_apply_atomically_and_idempotently` exécuté contre PostgreSQL 16.4 jetable : création de l’événement de modification, acceptation centrale, répétition reconnue comme doublon et données persistées à la révision 2. |
| Modification centrale et conflit de révision | PASS | Le même test met à jour le point de service central à la révision 3 et confirme qu’une modification concurrente avec révision attendue 2 est rejetée. L’audit et `central_changes` sont dans la transaction métier. |
| Contrat et contrôle d’accès | PASS local | OpenAPI documente GET/PATCH, ETag/If-Match et les champs éditables ; le test de matrice de rôles autorise la lecture aux auditeurs, le PATCH aux rôles métier déclarés et le refuse aux auditeurs. |
| Format, analyse statique et suite workspace | PASS local | `cargo fmt --all -- --check`, `git diff --check`, `cargo clippy --workspace --all-targets -- -D warnings` et `cargo test --workspace` réussissent. Les tests PostgreSQL nécessitant `TEST_DATABASE_URL` sont ignorés dans cette dernière commande ; le test PostgreSQL de ce lot a été exécuté séparément sur la base jetable. |
| CI GitHub du commit `1df34e46bc563e54472aca8a1b6ab75c87e6491d` | PASS | Backend : [run 41801907409](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41801907409) ; build Windows Desktop : [run 41801907483](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41801907483) ; paquet serveur : [run 41801907418](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41801907418) ; candidat Windows : [run 41801907313](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41801907313). Les quatre workflows sont terminés `success` sur ce SHA. |
| Utilisation dans le nouveau client Windows et recette sur poste SEL | NOT_STARTED / BLOCKED_EXTERNAL | Les API sont prêtes pour ce parcours, mais l’écran V2 de gestion des abonnés et la recette sur poste SEL ne sont pas encore réalisés. |

Le test HTTP local a été exécuté avant correction et échouait avec HTTP 404 ; après ajout de la route, il réussit. Toutes les données utilisées sont des fixtures et une base PostgreSQL éphémère ; aucune donnée SEL réelle n’a été lue ou modifiée.

**Progression estimée : Phase 2 100 % ; Phase 3 77 %.** Le lot « modification des points de service et synchronisation » est validé sur SQLite et PostgreSQL, et les quatre workflows GitHub du commit `1df34e46bc563e54472aca8a1b6ab75c87e6491d` sont réussis. Les parcours UI du nouveau frontend, PDF/impression, administration visuelle, autres besoins CRUD et recette matérielle SEL restent ouverts. Le pourcentage est une estimation des critères Phase 3 et ne signifie pas que le produit est prêt pour pilote ou production.

### Phase 3 — dossier financier client complet et validation de sortie backend (4 novembre 2026)

Le relevé client exposait auparavant seulement les créances. Les réponses locale et centrale incluent maintenant, en plus de ce champ rétrocompatible, les 100 factures les plus récentes du client et les 100 paiements les plus récents, avec leurs affectations, échéances éventuelles et contre-écritures append-only. Les montants viennent des snapshots et tables financières conservés ; aucun montant n'est recalculé côté interface. Les lectures sont bornées et les affectations/contre-écritures sont chargées en lots pour éviter une requête par paiement. Le contrat OpenAPI décrit la structure de réponse.

| Critère | Statut | Preuve |
|---|---|---|
| Reproduction du manque du dossier client | PASS (RED) | L'assertion du test HTTP local échouait sur l'absence de `invoices` dans la réponse `statement` avant l'implémentation. |
| Dossier local compatible | PASS | `cargo test -p sel-local --test local_commercial local_commercial_operations_are_persisted_and_offline_pending -- --exact` ; le dossier conserve les 2 créances attendues et présente la facture, le paiement et l'affectation correspondante. |
| Dossier central réel | PASS | `utility_issue_is_atomic_idempotent_and_survives_restart` exécuté contre PostgreSQL 16 jetable via l'API HTTP ; facture et paiement affecté apparaissent dans le dossier. |
| CRUD client central, relevés et synchronisation | PASS | `postgres_customer_api_reads_from_postgres_after_new_app_state`, `meter_readings_are_persisted_scoped_and_corrected_append_only_over_http` et `offline_meter_account_and_reading_events_apply_atomically_and_idempotently` exécutés contre le même PostgreSQL jetable. |
| Convergence commerciale multi-postes | PASS | `two_sqlite_devices_converge_through_real_postgres_and_http` exécuté : deux SQLite distinctes convergent via HTTP et PostgreSQL, sans doublon sur les opérations couvertes. |
| Contrat, format et analyse statique | PASS local | Test `openapi_contract`, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` et `git diff --check`. |
| Suite workspace | PASS local | `cargo test --workspace` : tous les tests actifs réussissent. Les tests PostgreSQL marqués `ignored` sont exclus de cette commande et seuls les parcours cités ci-dessus sont revendiqués comme exécutés manuellement contre le service jetable. |
| CI GitHub du commit `1dd7dff63561436b7c41098a48a7fcf0d6e09b32` | PASS | Quatre workflows terminés avec succès : [SEL V2 Backend](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41808613251), [Build Windows Desktop](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41808613181), [SEL V2 server package](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41808613185) et [SEL V2 Windows candidate](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41808613171). Le candidat Windows EXE/MSI/bootstrapper et ses empreintes ont été générés et téléversés comme artefact de test, sans publication de release. |
| Client Windows complet et recette SEL | NOT_TESTED / BLOCKED_EXTERNAL | Le client visuel complet appartient à la phase 7 ; aucun écran complet ni poste Windows/imprimante SEL n'a été validé par ce lot. |

Les tests PostgreSQL ont utilisé `postgres:16-alpine` dans un conteneur supprimé après validation et des organisations/clients générés pour les tests. Aucune donnée, facture, identité ou configuration réelle SEL n'a été lue ou modifiée. Aucun fichier de facture n'a été ajouté au stockage central.

**Progression de phase : Phase 2 backend 100 % ; Phase 3 backend 100 % pour les critères API de cette phase.** Les tests locaux, SQLite, PostgreSQL réel jetable, OpenAPI, convergence multi-postes couverte et les quatre workflows GitHub du commit `1dd7dff63561436b7c41098a48a7fcf0d6e09b32` réussissent. Cette clôture porte uniquement sur la phase backend clients/services ; elle ne couvre pas le nouveau frontend (phase 7), les documents PDF côté client (phase 8), les parcours complets de paiements et d’échéanciers à réaliser dans leurs phases dédiées, ni la recette matérielle et métier SEL (phase 10). Le système complet n’est donc pas déclaré prêt pour pilote ou production.

### Phase 4 — journal des paiements et données locales de reçu (4 novembre 2026)

Le backend expose maintenant un journal de paiements borné et filtrable par client/statut côté PostgreSQL central et SQLite locale. Les lectures centrales restent limitées à l’organisation du store ; les listes locales sont également filtrées par organisation. Le relevé client existant reste la source canonique des créances, affectations et soldes, sans création d'une deuxième projection de créances. Une route `payment document-data` versionnée fournit au client les faits du règlement, ses affectations et ses contre-écritures afin de générer un reçu sur le poste. Le serveur ne produit et ne stocke aucun PDF. L'identité et les coordonnées du client dans ces données de reçu proviennent de sa fiche au moment de la lecture ; les faits de paiement, montants, affectations et contre-écritures proviennent du journal financier.

| Critère | Statut | Preuve |
|---|---|---|
| Journal local filtré et borné | PASS local | `cargo test -p sel-local --test local_commercial local_commercial_operations_are_persisted_and_offline_pending -- --exact` : le paiement du client/statut demandé est retrouvé, puis relu après réouverture de SQLite ; statut inconnu rejeté. |
| Données de reçu locales et corrections | PASS local | Le test HTTP vérifie schéma v1, type reçu, identité et montant, affectations, contre-écriture et disponibilité après redémarrage SQLite. |
| Journal et reçu centraux | PASS PostgreSQL réel jetable | `utility_issue_is_atomic_idempotent_and_survives_restart` : routes HTTP authentifiées, filtre client/statut, reçu, affectations et contre-écritures, puis persistance après reconnexion du store. PostgreSQL 16, base temporaire locale, fixtures synthétiques. |
| Isolation des organisations | PASS PostgreSQL réel jetable | `payment_journal_and_receipt_are_scoped_to_the_store_organization` : journal limité à une organisation et reçu d'une autre organisation introuvable. |
| Invariants d'allocation et concurrence | PASS PostgreSQL réel jetable | `duplicate_allocations_are_aggregated_before_balance_validation`, `concurrent_payments_cannot_both_consume_the_same_balance` et `post_record_payment_allocation_is_atomic_idempotent_and_grouped` : pas de surallocation par affectations répétées, un seul encaissement concurrent accepté, idempotence et écritures attendues. |
| Relevés, soldes, corrections append-only | PASS SQLite / PostgreSQL réel jetable | Tests commerciaux vérifient les soldes après paiement partiel, affectations, renversements et note de crédit ; aucun échéancier ordinaire n'est créé automatiquement. |
| RBAC et OpenAPI | PASS local et CI finale | La matrice autorise les lectures de paiement aux rôles lecteurs et refuse leurs écritures. Le test OpenAPI était passé avant l'ajout d'une description puis avait détecté une indentation YAML invalide ; l'indentation a été corrigée et le workflow Backend final a validé le YAML et le contrat sur le SHA ci-dessous. |
| Suite Rust workspace | PASS CI finale | `cargo test --workspace` local s'est terminé avec exit code 0 avant les dernières assertions de statut/documentation. Le workflow Backend a ensuite validé le commit complet. Les tests PostgreSQL ignorés dans la suite ordinaire ont été lancés séparément sur PostgreSQL 16 jetable, comme détaillé ci-dessus. Aucun test local supplémentaire n'a été lancé après le signalement de surchauffe. |
| Format / diff | PASS local et CI finale | `cargo fmt --all -- --check` et `git diff --check` locaux ont réussi avant les derniers changements de test/documentation ; `cargo fmt --all` a été appliqué ensuite. Le workflow Backend final valide les sources et contrats du commit final. |
| CI GitHub du commit `ded43071d132d764c127dda7ca704ef601c93589` | PASS | [SEL V2 Backend — run 41813117730](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41813117730), [paquet serveur — run 41813117322](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41813117322), [Build Windows Desktop — run 41813117410](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41813117410) et [Windows candidate — run 41813117262](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41813117262) ont tous terminé `success`. |
| PDF/reçu imprimable et parcours d'interface Windows | NOT_STARTED dans Phase 4 | Les données structurées sont disponibles ; la mise en page, l'export, l'impression et les écrans relèvent des phases frontend/documents. Aucun reçu PDF n'est encore revendiqué. |
| Recette sur poste SEL | BLOCKED_EXTERNAL | Nécessite le nouveau client, un poste représentatif SEL et l'acceptation métier ; aucun système ni donnée réels n'a été modifié. |

Les tests PostgreSQL ont tourné sur `postgres:16-alpine` dans un conteneur temporaire et une base de test jetable. Le conteneur a été supprimé après exécution. Aucun secret, aucune facture PDF et aucune donnée SEL réelle n'ont été utilisés. Après le signalement d'une surchauffe matérielle, aucune nouvelle compilation locale ne sera lancée pour cette phase ; la validation des derniers changements dépend de la CI distante.

**Progression de Phase 4 : 100 % pour le périmètre backend/API défini dans `PHASE_4_PAIEMENTS_PLAN.md`**, validé par tests ciblés, PostgreSQL réel jetable et CI finale. Les reçus restent des données JSON destinées au rendu local : leur mise en page, export et impression sont traités dans les phases frontend/documents. La fiche client des reçus reflète le profil au moment de la lecture, tandis que les faits financiers sont issus du journal. Les remboursements ou compensations métier non spécifiés restent à confirmer par SEL. Cela ne clôt pas les phases frontend, documents, recette sur poste SEL, ni la préparation opérationnelle.

### Phase 5 — convergence offline multi-postes (clôturée pour les validations automatisables, 4 novembre 2026)

L’audit confirme que le test ignoré `two_sqlite_devices_converge_through_real_postgres_and_http` est déjà exécuté dans le job CI PostgreSQL (`cargo test --workspace -- --ignored`). Il couvre la facture eau + électricité et son paiement partiel sur le premier poste, son redémarrage avant envoi, la convergence vers PostgreSQL puis le second poste, la vente solaire avec échéancier, ainsi que deux paiements concurrents sur la même obligation. La suite `sync_worker.rs` couvre séparément les reprises, les projections et les conflits avec un serveur simulé.

Le nouvel incrément étend le scénario réel multi-postes : un middleware de panne remplace par HTTP 503 la première réponse du vrai `POST /v1/sync/push`, après que la route PostgreSQL a terminé sa transaction. Le test vérifie que les événements sont déjà inscrits dans `sync_inbox`, que l'outbox SQLite reste à transmettre, redémarre le premier store et rejoue les événements. Il compare le nombre d'événements et de changements centraux avant/après rejeu, puis vérifie les soldes de la facture et du contrat solaire dans les deux SQLite et PostgreSQL. Cette injection simule la perte de l'accusé applicatif après commit ; elle ne prétend pas reproduire une coupure TCP réelle.

| Critère | Statut | Preuve |
|---|---|---|
| État initial et parcours existant | PASS code/CI antérieure | Le test multi-postes est exécuté par la CI avec PostgreSQL ; les scénarios de reprise et de rejeu du worker sont dans `sync_worker.rs`. |
| Rejeu après commit central sans accusé | COMPLETED (CI) | Sur `45f1346fceec7c8b186fa072ad1997138f5100da`, la première réponse du vrai endpoint HTTP est remplacée par 503 après commit PostgreSQL ; après redémarrage SQLite, le rejeu renvoie 13 doublons sans regonfler `sync_inbox` ni `central_changes`. Le test `two_sqlite_devices_converge_through_real_postgres_and_http` passe dans le job PostgreSQL. |
| Égalité des factures, paiements et soldes sur trois bases | COMPLETED (CI) | Le test multi-postes exécuté sur PostgreSQL réel compare identifiants, factures, lignes et créances ; les soldes de paiement partiel concordent dans PostgreSQL et les deux SQLite. |
| Échéancier solaire et paiements concurrents | COMPLETED (CI) | Le scénario vérifie la vente/contrat/échéancier, deux demandes concurrentes sur la même obligation, un paiement accepté et l'autre refusé, l'unicité du paiement central et l'égalité des soldes sur les trois bases. |
| Coupure réseau réelle/Windows et détection de reconnexion | BLOCKED_EXTERNAL | Le test ajoute une réponse HTTP de panne après commit ; le reset réseau, le signal système Windows et l'essai sur poste SEL ne sont pas réalisés. |

**Progression Phase 5 : 100 % pour la convergence automatisable dans le dépôt.** La preuve finale porte sur le commit code `45f1346fceec7c8b186fa072ad1997138f5100da` : [SEL V2 Backend — run 41815388416](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41815388416) (formatage, Clippy, `cargo test --workspace` et tests `--ignored` sur PostgreSQL réel), [paquet serveur — run 41815388430](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41815388430), [Build Windows Desktop — run 41815388310](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41815388310) et [Windows candidate — run 41815388369](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41815388369) ; les quatre sont `success`. La perte d'accusé est injectée en HTTP après commit et le scénario multi-postes est réellement exécuté. La coupure TCP réelle, la détection de reconnexion Windows et la recette sur le poste SEL restent **BLOCKED_EXTERNAL** et ne sont pas incluses dans cette clôture automatisée.

### Phase 6 — backend kits solaires (clôturée, 4 novembre 2026)

L'audit a confirmé les composants déjà disponibles : catalogue versionné avec approbation/révocation, devis dérivés du prix approuvé, vente hors ligne idempotente, création de contrat et échéancier dont la somme correspond au principal, lecture des soldes/retards, paiements affectés à une échéance, projections SQLite et synchronisation centrale. Le test multi-postes réel de la Phase 5 couvre une vente solaire, son échéancier, la concurrence de paiements et la convergence des soldes.

L'écart identifié concernait l'historique local : après activation d'un contrat, `GET /local/v1/solar/sales` renvoyait l'identifiant du contrat mais pas celui du plan, empêchant l'interface d'ouvrir directement l'échéancier depuis la vente. Le test `local_solar_sale_and_contract_are_persisted_offline` a d'abord échoué sur cette valeur absente au commit de test `89c719daa22e0600e16467662875cb9de367b7b9` (run Backend 41816572498), puis le stockage a ajouté la jointure vers `installment_plans` et `installment_plan_id` à la réponse locale dans `ceb5e44a6f04364cb14d3f1b7d99868759055e10`.

| Critère | Statut | Preuve |
|---|---|---|
| Catalogue solaire versionné et prix approuvés | COMPLETED (API/CI) | Tests de catalogue PostgreSQL : création, approbation, révocation et dates d'effet ; devis central/local et rejet d'un prix divergent. |
| Vente, contrat et échéancier hors connexion | COMPLETED (API/CI) | `local_solar_sale_and_contract_are_persisted_offline` passe ; écritures et outbox locale sont atomiques. |
| Paiements affectés, soldes et retards | COMPLETED (API/CI) | Routes de plan et d'affectation, calcul des retards/soldes, tests PostgreSQL et scénario multi-postes de Phase 5. |
| Consultation d'un plan depuis l'historique des ventes | COMPLETED (CI) | La réponse locale des ventes expose `contract_id` et `installment_plan_id`; le test lit le plan via l'identifiant de la vente après activation. |
| Synchronisation, idempotence et concurrence | COMPLETED (CI) | Le scénario réel deux SQLite + PostgreSQL vérifie contrat/échéancier, paiement accepté/refusé sur concurrence et absence de duplication après rejeu. |
| Écrans Windows solaires et documents remis au client | NOT_STARTED (phases 7/8) | Les routes et données sont disponibles, mais le nouveau frontend et les documents locaux seront traités dans les phases dédiées. |
| Recette sur poste SEL | BLOCKED_EXTERNAL | Aucun poste Windows SEL, imprimante ni jeu d'acceptation métier n'a été utilisé dans ces tests CI. |

La validation finale du commit `ceb5e44a6f04364cb14d3f1b7d99868759055e10` est verte : [SEL V2 Backend — run 41817030731](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41817030731), [paquet serveur — run 41817030886](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41817030886), [Build Windows Desktop — run 41817030930](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41817030930) et [Windows candidate — run 41817030806](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41817030806) ont terminé `success`. Cela valide le code et la construction des paquets, pas l'installation ni l'acceptation utilisateur sur le poste SEL.

**Progression Phase 6 : 100 % pour le périmètre backend solaire.** L'interface et les documents client sont explicitement suivis en phases 7 et 8 ; ils ne sont pas déclarés livrés par cette clôture.
