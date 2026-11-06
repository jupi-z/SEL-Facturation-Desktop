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

### Phase 7 — frontend Windows V2 conforme au fichier Figma (en cours, 4 novembre 2026)

L'audit de conception a porté sur la page Figma « Windows / Core operations » : tableau de bord (`7:2`), répertoire (`7:96`), dossier abonné (`7:188`), nouvelle facture (`7:282`), prévisualisation (`7:357`), encaissement (`7:433`), historique (`7:508`), compteurs (`7:602`), relevé d'eau (`51:2`) et facture avec eau consommée (`51:85`). Les tokens typographiques/fonctionnels des fondations (`4:2`) et les composants (`6:2`) ont également été consultés. Le frontend reste une application HTML/CSS/JavaScript intégrée à pywebview ; cette tranche n'ajoute ni React, ni Tailwind, ni ressource chargée depuis Internet.

| Critère | Statut | Preuve / limite |
|---|---|---|
| Audit des composants communs Figma | PARTIAL | Les vues de la page Core operations listées ci-dessus, les tokens et composants communs ont été inspectés. Solaire, créances, administration et documents restent à auditer. |
| Shell Windows aligné aux fondations | PARTIAL (CI) | `v2/client/index.html` et `v2/client/app.css` reprennent `#012A5E`, `#FDDC02`, le rail de 250 px, le trait de 7 px, le bleu d'action `#075BBB`, Geist et la grille de base. Aucun rendu de l'application n'a été vérifié visuellement. |
| Marque et typographie locales | PASS (CI) | Le logo officiel est local dans `v2/client/assets/sel-logo-dark.svg`. Geist variable est local dans `v2/client/assets/geist-variable.woff2`; licence et provenance sont jointes. Le test `test_v2_client_assets_are_complete_and_local` passe. |
| Navigation accessible | PASS (CI contrat) | Les six destinations fonctionnelles exposent l'état courant via `aria-current`; test `test_v2_navigation_uses_local_icons_and_accessible_current_state`. Créances et Administration ne sont pas présentées comme opérationnelles avant implémentation. |
| Empaquetage des ressources | PASS (CI build) | Le workflow `build-candidate` a compilé l'EXE, MSI, bootstrapper et empreintes; `desktop_v2.py` contrôle sept ressources, incluses depuis le dossier client empaqueté. L'installation sur poste SEL reste à valider. |
| Vérification de code locale | PASS limité | `git diff --check` ne signale aucun espace blanc invalide. Aucun test Python, build, lancement de WebView ou capture locale n'a été exécuté pour protéger le poste contre la surchauffe. |
| CI après cette tranche | PASS | Commit `414456d040a6c3db0b513085a7bde428ea5d1ed6` : [desktop tests 41820265037](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41820265037), [Windows candidate 41820265146](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41820265146), [Rust 41820265039](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41820265039) et [server package 41820265017](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41820265017) — tous `success`. |
| Recette visuelle sur Windows SEL | BLOCKED_EXTERNAL | Nécessite un poste représentatif et une capture réelle de l'application. |

Les valeurs de démonstration visibles sur les écrans Figma ne sont pas reprises dans l'application. Le bandeau d'état indique uniquement l'état du service local et le nombre d'opérations en attente ; il ne prétend pas que le serveur est en ligne ou que les données sont synchronisées. Cette première tranche ne clôt pas la phase 7 : les écrans métier détaillés, le catalogue/contrat solaire, les créances et l'administration restent à intégrer.

**Progression Phase 7 : 27 %** (audit Figma des écrans Core operations et shell/navigation validés par CI; aucun parcours Windows n'est déclaré validé visuellement). Les écrans détaillés, tests de comportement et recette visuelle restent nécessaires.

### Phase 7 — mise à jour d’architecture et tranche Tauri/React (4 novembre 2026)

La section précédente est conservée comme état historique de la tranche pywebview. Conformément au plan approuvé de reconstruction du frontend, le travail courant se fait dans `v2/windows-app/` en Tauri 2 + React/TypeScript ; l’ancienne interface n’est pas reprise dans la nouvelle application. Le frame Figma `7:2` a été relu pendant cette tranche. La grille 1440×900, sidebar 250 px, barre haute 76 px, couleurs de marque, Geist locale, trait supérieur jaune, navigation et marque sont repris dans les styles React/CSS. Les carrés d’icône reprennent les composants de navigation visibles dans la maquette au lieu d’icônes décoratives d’un autre style.

| Critère | Statut au commit `03d0f9d` | Preuve / limite |
|---|---|---|
| Nouveau shell desktop Tauri/React, sans dépendance runtime distante | IN_PROGRESS | `v2/windows-app/`; contrat et assets locaux couverts par `tests/test_v2_tauri_client.py`. |
| Correspondance des fondations avec Figma 7:2 | PARTIAL | Tokens, dimensions principales, logo, typographie, navigation, rail jaune et activité récente implémentés. Aucun screenshot comparatif de l’application compilée n’est encore produit. |
| Tableau de bord | PARTIAL | Compteurs issus des données locales et tableau récent combinant factures, paiements et ventes ; aucune donnée de démonstration ou agrégation financière inventée. Le KPI financier Figma nécessiterait un agrégat backend dédié. |
| Abonnés | PARTIAL | Liste/recherche, création, fiche, relevé et modification avec If-Match vers l’API locale. Champs absents du contrat Rust ne sont pas prétendus modifiables. |
| Factures eau + électricité | PARTIAL | Prévisualisation Rust avant émission, confirmation distincte, historique et lecture du détail sans bouton de réémission. Le choix de la révision tarifaire est encore saisi manuellement ; UX/API du catalogue de tarifs à compléter. |
| Encaissements et créances | PARTIAL | Saisie/affectation à une créance proposée par l’API et consultation des soldes ; parcours d’affectations multiples, annulations et reçus d’interface incomplets. |
| Vente solaire | PARTIAL | Catalogue local actif/approuvé, devis Rust, confirmation explicite et création de vente. Contrats/échéanciers et administration visuelle du catalogue restent à intégrer. |
| Compteurs, relevés, corrections | NOT_STARTED (UI) | Les routes locales existent, mais aucun parcours de saisie/consultation des compteurs n’est encore livré dans React. |
| Synchronisation | PARTIAL | État et demande de synchronisation manuelle reliés à l’agent ; liste détaillée des événements/conflits et résolution restent à construire. |
| Administration | BLOCKED_BY_API_GAP | L’écran signale explicitement les opérations non exposées au client local ; aucun faux contrôle actif. |
| Bridge Tauri et sidecar Windows | IN_PROGRESS | IPC restreint aux routes locales, secret conservé dans Rust, démarrage fail-closed et packaging de `sel-local.exe`. Le workflow a révélé un dossier de ressources absent, corrigé au commit `03d0f9d`; le nouveau run est encore en cours. |
| CI sur le dernier commit | IN_PROGRESS | À `03d0f9d`, les cinq workflows GitHub sont lancés. Au commit précédent `c20f533`, backend, paquet serveur et candidat Windows V1 ont réussi ; le workflow Tauri a échoué lors de la copie vers un dossier inexistant. Le correctif crée ce dossier et doit être confirmé par le run courant. |
| Recette visuelle Windows | BLOCKED_EXTERNAL | Aucun lancement/capture de l’application compilée ou test d’installation sur le poste SEL. Les captures/mesures prévues doivent être faites sur un runner ou un poste représentatif ; l’ordinateur de développement est préservé de compilations locales à cause de sa surchauffe. |

Les commits de cette tranche sont `fa051de` (fondation Tauri et premières routes catalogue), `68ecf50` (corrections CI), `0e43fa4` (fiches client et détail facture), `c20f533` (activité récente alignée sur Figma) et `03d0f9d` (dossier de ressources sidecar). Le statut de la génération du MSI/NSIS Tauri reste non confirmé tant que le workflow du dernier SHA n’est pas vert.

**Progression Phase 7 : 34 % d’implémentation estimée, non équivalente à une recette acceptée.** Le shell et les premiers parcours ont été développés, mais la CI Tauri courante, l’intégration des autres écrans Figma, les captures comparatives et la validation Windows restent à accomplir. Ce pourcentage ne signifie ni livraison ni validation sur poste SEL.

### Phase 7 — alignement des parcours desktop sur Figma (mise à jour du 4 novembre 2026)

La progression ci-dessous remplace le pourcentage historique de 34 % pour l’état courant ; les tableaux datés plus haut restent des traces de leurs anciens commits. Le client neuf est dans `v2/windows-app/` (Tauri 2, React/TypeScript). Les tests/captures Vite utilisent un pont Tauri simulé et des fixtures explicitement fictives : ils vérifient l’interface et les requêtes du client, pas les règles commerciales en production.

| Domaine Phase 7 | Statut courant | Code développé | Tests / preuves | Utilisable / validation restante |
|---|---|---|---|---|
| Shell, marque, navigation, ressources locales | PARTIAL | Rail de 250 px, barre de 76 px, couleurs SEL, logo, Geist, icônes locales, état courant accessible ; aucune ressource distante nécessaire à l’interface. | Contrats statiques dans `tests/test_v2_tauri_client.py`; 19 captures desktop à 1440×900 dans le workflow Tauri du commit `ea74ad078cfb046f2890b9a6b42dcec92a9b750c` (`41869074581`). | Le nouveau client se construit dans le workflow Tauri ; installation et DPI Windows réels restent non vérifiés. |
| Tableau de bord et abonnés | PARTIAL | Données locales réelles de l’agent, recherche, création, dossier, édition à révision contrôlée et quatre indicateurs sans chiffres métier fictifs dans l’application. | Scénarios de capture des vues et formulaires dans `v2/windows-app/scripts/visual_review.py`; tests client locaux. | Pas de suppression destructive de clients ; recette sur données SEL absente. |
| Relevés électricité et eau | IN_PROGRESS | Sélection abonné/service/compteur, saisie, aperçu validé par Rust, validation explicite, historique et correction auditée. Titre et unités s’adaptent à l’eau (`m³`) et à l’électricité (`kWh`). | Fixtures distinctes KWH/M3 et capture d’aperçu eau ; test Python ciblé `42 passed`. Revue Windows CI `41869074581` réussie sur `ea74ad0`; la capture d’eau figure dans l’artefact `SEL-V2-Figma-Desktop-Review-53`. | Aucun relevé de démonstration n’est créé dans le système ; aucun test matériel SEL. |
| Facturation eau + électricité | IN_PROGRESS | Étapes séparées saisie → aperçu → émission ; retour à la saisie conserve les valeurs. Aperçu alimenté par Rust et métriques formatées selon la maquette. Le cas 0 m³ reste explicite. | Captures 0 m³ et eau positive validées visuellement par le workflow Windows `41869074581` sur `ea74ad0`, y compris les unités et totaux des deux lignes. Données de démonstration synthétiques uniquement, sans validation de tarif ou fiscalité SEL. | PDF/impression appartiennent à la phase 8. Un essai de bout en bout sur poste représentatif via l’agent reste requis. |
| Encaissements, créances, ventes et échéanciers solaires | PARTIAL | Saisie de règlement affectée à une créance retournée par l’API ; catalogue local en lecture, devis Rust, vente, création/consultation d’échéancier. | Captures de formulaire, contrat, paiement et état solaire ; tests statiques des routes locales. | Affectations multiples, reçu, édition du catalogue et certains actes de gestion restent hors de l’interface. |
| Synchronisation | PARTIAL | État local, compteurs en attente/conflits/refus et demande de synchronisation ; les libellés distinguent l’état local d’un ACK central. | Revue d’écran synthétique 1440×900 et tests de contrat. | Pas de liste détaillée ni résolution de conflit dans le client ; le comportement réel multi-postes relève des phases backend et recette. |
| Administration | PARTIAL / API manquante | L’écran affiche explicitement les fonctions indisponibles ; il ne présente pas de faux boutons actifs pour utilisateurs, appareils, audit ou écriture des tarifs. | Capture synthétique de l’écran Administration. | Le service local n’expose pas ces opérations d’administration à l’application ; une intégration sécurisée nécessite des contrats et une autorisation centrale adaptés. |
| Comportement visuel, clavier et DPI | PARTIAL | Composants natifs accessibles par clavier, tailles desktop Figma, unités localisées, formulaires et états. | Revue visuelle automatisée ciblée à 1440×900 ; tests Python ciblés 42/42 et `git diff --check` passent localement. | Pas encore de matrice multi-résolution/DPI, lecteur d’écran ou recette sur l’exécutable installé. |
| Build Tauri, installateur et capture du commit courant | COMPLETED (CI) | Build et captures exécutés sur runners Windows GitHub ; aucune compilation locale lancée pour ménager le PC sujet à la surchauffe. | Sur `ea74ad078cfb046f2890b9a6b42dcec92a9b750c`, Tauri Windows `41869074581` a réussi : contrats, 19 captures, compilation agent Rust, client React et MSI/NSIS. Build Windows Desktop `41869074728`, backend `41869074531`, paquet serveur `41869074905` et candidat Windows `41869074633` ont également réussi. Artefact visuel `SEL-V2-Figma-Desktop-Review-53` (1 593 154 octets) téléchargé et inspecté localement sous `artifacts/ci-visual-ea74ad0/`. | Ces résultats attestent les builds et captures synthétiques ; ils ne remplacent ni l’installation native, ni la recette DPI/imprimante, ni les opérations métier sur poste SEL. |
| Poste SEL et mise en production | BLOCKED_EXTERNAL | Aucun changement apporté à des données ou installations réelles. | Aucune preuve de lancement natif, imprimante, données, serveur ni configuration réelle SEL. | Nécessite poste Windows représentatif, imprimante, identité/autorisation et paramètres métier confirmés. Rien n’est déclaré déployé. |

Les derniers incréments visuels sont `930c951` (indicateurs d’abonnés), `6459522` (actions contenues dans les panneaux), `360b2d4` (champs du règlement), `1cbd482` (cadrage de capture solaire), `c5f2a47` (aperçu de facture dédié), `6b7386b` (fixtures/captures eau), `c08ed26` (unités kWh/m³ et décimales françaises), `4191583` (assertion d’aperçu), `d905f81` (stabilisation de la revue visuelle) et `ea74ad0` (sélecteur borné aux métriques et preuve CI verte). Les captures ont été inspectées pour le tableau de bord, abonnés, relevés, les deux prévisualisations de facture, encaissements, créances, solaire, synchronisation et administration.

**Progression Phase 7 : 84 %.** Les parcours centraux et les captures de revue visuelle desktop Figma passent en CI Windows sur `ea74ad0`, et tous les workflows déclenchés par ce commit sont verts. L’administration reste limitée par l’absence d’API locale ; l’authentification réelle, les validations DPI, l’installation native, l’acceptation métier et la recette sur poste SEL ne sont pas effectuées. Ce pourcentage mesure l’implémentation et les tests démontrés, pas l’acceptation ni la livraison de la phase.

### Phase 7 — raccordement sécurisé de l’administration (4 novembre 2026)

| Domaine | État | Implémentation / preuve | Limite restante |
|---|---|---|---|
| Session centrale nominative | IN_PROGRESS | Connexion HTTPS via Tauri natif ; jetons d’accès/renouvellement conservés uniquement en mémoire native ; état de session sans exposition des jetons au JavaScript ; renouvellement après expiration et déconnexion révocable côté API. Le jeton de synchronisation de l’appareil n’est pas réutilisé pour l’administration. | La connexion réelle doit être validée avec une configuration et un compte administrateur SEL autorisés. |
| Pont de requêtes administratives | IN_PROGRESS | Liste blanche de routes, méthodes et identifiants UUID ; refus des query strings, encodages et chemins traversants ; rôle ADMIN contrôlé localement et revalidé par l’API centrale. Tests Rust adversariaux ajoutés, exécution Windows CI en attente de ce commit. | Le journal d’audit n’a pas de route de lecture centrale ; aucune route libre n’est relayée. |
| Gestion des utilisateurs | IN_PROGRESS | Création, changement de rôle, activation/désactivation et réinitialisation de mot de passe depuis l’écran ; secrets transmis uniquement au canal HTTPS et hashés par le serveur ; permissions, protection du dernier administrateur, audit et révocation des sessions demeurent appliqués par l’API centrale. | Gestion détaillée des sessions actives non intégrée à l’écran. |
| Appareils, autorisations et référentiels | PARTIAL | Listes centrales des appareils, grants, tarifs, produits, profils fiscaux et taux FX ; révoquer, créer et approuver des référentiels n’est pas encore exposé par l’interface. | Les formulaires d’édition et d’approbation des tarifs, taxes, taux FX et produits restent à intégrer ; aucune valeur métier n’est préremplie ou modifiée. |
| Revue visuelle d’administration | IN_PROGRESS | Deux états ajoutés au script de revue : connexion non authentifiée et console authentifiée alimentée par données synthétiques identifiées comme démo ; capture longue de la console. | Capture Windows CI attendue ; cette simulation ne valide pas les droits ni les données réelles. |

Vérifications locales de cette tranche : `python -m pytest tests/test_v2_tauri_client.py -q` — **45 réussis** ; `git diff --check` — réussi. La compilation Tauri, les tests Rust natifs, les captures et le paquet Windows n’ont pas été exécutés sur ce PC afin d’éviter une nouvelle charge thermique ; le workflow Windows dédié a été étendu pour exécuter les tests Rust après la construction React. Ses résultats seront consignés au commit final.

**Progression Phase 7 réévaluée : 89 % d’implémentation.** Le frontend dispose maintenant d’une console administrative réellement connectée et protégée par une session nominative. La phase reste `IN_PROGRESS` : l’écriture/approbation visuelle des référentiels, le détail/résolution des conflits de synchronisation et certaines validations d’accessibilité/DPI restent à faire. La validation sur poste installé, avec compte SEL et imprimante, est `BLOCKED_EXTERNAL`. Ce pourcentage mesure les écrans et intégrations vérifiables, pas l’acceptation ni le déploiement.

### Phase 7 — formulaires d’administration des référentiels et clés d’idempotence (4 novembre 2026)

| Domaine | État | Preuve / limite |
|---|---|---|
| Administration des utilisateurs | IN_PROGRESS | Session centrale nominative, création, rôle, activation/désactivation et réinitialisation du mot de passe. Les jetons restent côté Rust et ne sont pas persistés côté interface. Tests client ciblés : `tests/test_v2_tauri_client.py`. | Les droits effectifs et les actions doivent être validés avec une identité SEL autorisée ; la route de consultation d’audit n’existe pas. |
| Tarifs, produits solaires, profils fiscaux et devises | IN_PROGRESS | Écrans de création de brouillon, approbation avec référence SEL explicite, remplacement optionnel d’une version et révocation avec référence. Les valeurs restent vides par défaut ; le serveur central demeure l’autorité et refuse les données invalides/chevauchantes. | Aucun tarif, taux fiscal ou change réel n’a été saisi ou approuvé. Le poste doit être enrôlé et connecté au serveur central. |
| Pont natif et idempotence | IN_PROGRESS | Liste blanche native des routes/méthodes/UUID ; clés `Idempotency-Key` générées pour les écritures HTTP du pont ; tests d’autorisation et d’entrées adversariales ajoutés. Le workflow Windows exécute les tests Rust de la bibliothèque. | Tests Rust/compilation/build MSI/NSIS sur le runner CI requis avant de déclarer ces changements validés. |
| Vérification locale ciblée | PASS limité | `python -m pytest tests/test_v2_tauri_client.py -q` : **46 réussis** ; `git diff --check` : PASS. | Aucun build React/Rust local ni test sur Windows réel lancé sur cette machine, afin d’éviter la charge thermique déjà signalée. |
| Conflits et journal d’audit dans l’interface | PARTIAL / API manquante | Le client affiche les compteurs de synchronisation exposés par l’agent ; aucune route locale ne fournit la liste détaillée des conflits et leur résolution, et l’API centrale ne fournit pas de lecture du journal d’audit. Aucun faux bouton de résolution n’est présenté. | Exige des contrats API dédiés et leurs règles métier avant ajout d’actions d’interface. |
| Recette DPI/accessibilité et poste SEL | BLOCKED_EXTERNAL | Les états de revue visuelle synthétique sont préparés dans `v2/windows-app/scripts/visual_review.py`. | La validation d’installation, de mise à l’échelle, de clavier/lecteur d’écran, de compte et d’impression sur un poste représentatif reste à réaliser. |

Le pont natif utilise TLS via `reqwest` avec `rustls`; les opérations d’administration emploient uniquement la session nominative distincte du jeton de synchronisation appareil. Les répertoires centraux s’écrivent en deux étapes visibles : création du brouillon, puis approbation ou révocation avec une référence fournie par l’opérateur autorisé. La prise en charge des clés d’idempotence évite de transformer une répétition réseau en écriture nouvelle lorsque l’API centrale applique son contrat correspondant.

**Progression Phase 7 actualisée : 92 % d’implémentation.** Les formulaires des référentiels encore manquants sont maintenant raccordés aux routes centrales existantes. La phase reste `IN_PROGRESS` jusqu’aux résultats CI Windows du nouveau commit et à la livraison des API de consultation/résolution des conflits et d’audit. La recette réelle du compte, de l’installation, du DPI et de l’impression reste `BLOCKED_EXTERNAL`. Ce pourcentage ne signifie ni validation sur poste SEL, ni déploiement.

### Phase 7 — validation du client et de l’administration Tauri (4 novembre 2026)

Les changements applicatifs de cette tranche ont été validés par les workflows GitHub Windows et backend sur le commit de code `a39e388fdd2567ddab7d798912dafede938e6870` (`feat(v2): complete central admin catalog workflows`, complété par deux correctifs de cible TypeScript et de dépendance Tokio). La console centrale utilise une session nominative détenue par Rust, des routes/méthodes allowlistées, la validation du rôle par le serveur, et des clés d’idempotence pour les écritures. Elle permet les opérations utilisateurs et référentiels documentées ci-dessus ; les valeurs financières restent saisies et approuvées par SEL.

| Vérification | Statut | Preuve exacte / limite |
|---|---|---|
| Tests client ciblés | PASS | `python -m pytest tests/test_v2_tauri_client.py -q` : 46 réussis sur le poste de développement. |
| Métadonnées Cargo et espace de travail | PASS limité | `cargo metadata --manifest-path v2/Cargo.toml --locked --no-deps --format-version 1` : réussi ; ce n’est pas une compilation locale. |
| Contrôle de diff | PASS | `git diff --check` : réussi avant les commits de code. Aucun build local Rust/React n’a été exécuté pour éviter la surchauffe du PC. |
| CI Tauri Windows | PASS | [run 41887329638](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41887329638) : tests client, installation des dépendances, revue de captures à 1440×900, compilation agent, build React, tests Rust natifs, paquets MSI et NSIS et publication de l’artefact candidat réussis. |
| Autres workflows du commit | PASS | Backend Rust [41887329609](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41887329609), candidat Windows [41887329670](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41887329670), paquet serveur [41887329650](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41887329650) et ancien client Windows [41887329610](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41887329610) : tous `success`. |
| Candidat Tauri Windows | PASS build / NON RECETTÉ | Artefact `SEL-V2-Tauri-Windows-57`, 13 031 915 octets, SHA-256 `66db91f6a2656cdd5e14ca3117fb4a1b179ca702dfa34f4bb9f3217295efe99f`, expiration 2027-02-02. Il s’agit d’un candidat de CI, pas d’un installateur testé sur le poste SEL. |
| Captures de revue | PASS pipeline / NON ACCEPTÉ visuellement | Artefact `SEL-V2-Figma-Desktop-Review-57`, 1 849 719 octets, SHA-256 `565d2ca0b39c1d05f2152fc8727b3ec434cca22f0ab7e46c7d8d8efa9c82f24d`, expiration 2026-11-18. Les captures de revue automatisée ne valent pas recette utilisateur. |
| Dépôt et PR | PASS état observé | Branche `monarque/sel-v2-backend`, commit de code `a39e388`; PR [#58](https://github.com/jupi-z/SEL-Facturation-Desktop/pull/58) ouverte et non fusionnée. `main` n’a pas été modifiée. Le dossier utilisateur non suivi `artifacts/` a été préservé. |
| Recette SEL réelle | BLOCKED_EXTERNAL | Aucun compte administrateur SEL, serveur réel, imprimante ni poste représentatif n’a été utilisé. Aucun artefact n’a été installé ou déployé. |

Les API de lecture du journal central et de détail/résolution des conflits ne sont pas présentes dans le contrat courant. La résolution des conflits financiers nécessite des règles métier explicites et ne sera pas simulée par le client. Les parcours Windows restent partiels pour les relevés, les multi-affectations et certains actes de gestion ; les documents et l’impression relèvent de la phase 8. Ces lacunes empêchent de déclarer la phase 7 entièrement close malgré la CI verte.

**Progression Phase 7 : 93 % d’implémentation vérifiée par code et CI. Statut : `IN_PROGRESS`.** La nouvelle console d’administration, le pont natif, les captures automatisées et les paquets Windows sont maintenant validés en CI. Les 7 % restants comprennent les parcours métier encore partiels, les contrats API d’audit/conflits, et la recette Windows/SEL externe ; aucun de ces points n’est compté comme terminé. Le commit `a39e388` est le dernier commit applicatif couvert par les cinq workflows verts. Cette mise à jour documentaire ne change pas le code testé.

### Phase 7 — diagnostic de synchronisation et audit administrateur (4 novembre 2026)

Cette tranche complète les lacunes de visibilité et de contrôle identifiées dans l’administration et le parcours de synchronisation, sans inventer de résolution automatique des conflits financiers.

| Domaine | Statut | Changement et preuve | Limite restante |
|---|---|---|---|
| Diagnostics locaux de synchronisation | IN_PROGRESS — code et test ajoutés | `GET /local/v1/sync/issues` fournit jusqu’à 100 conflits/refus avec événement, type de commande, motif, révision centrale et date. La requête ne retourne jamais le payload métier. Les compteurs `/sync/status` viennent maintenant des tables durables et dédupliquent les conflits déjà inscrits dans les deux sources. Test HTTP ajouté dans `v2/crates/sel-local/tests/local_http.rs`. | L’exécution du test Rust attend la CI ; les opérations financières restent conservées et ne sont pas résolues automatiquement sans règle approuvée. |
| Écran de synchronisation | IN_PROGRESS — code et contrat statique validé | `v2/windows-app/src/App.tsx` charge et affiche les motifs des conflits/refus conservés localement. Le libellé précise qu’aucune résolution financière automatique n’est effectuée. Le scénario synthétique de revue visuelle comprend un conflit et un refus. | Le test client statique passe localement ; rendu réel à confirmer par la CI Windows de ce commit et validation opérateur SEL absente. |
| Journal d’audit central | IN_PROGRESS — code, contrat et test ajoutés | `GET /v1/admin/audit-events` est réservé au rôle ADMIN, limité à 100 événements récents associés à l’organisation par leur acteur ou appareil et ne sélectionne que les métadonnées déjà expurgées. Contrat ajouté à `v2/openapi/sel-v2.yaml`; test PostgreSQL d’intégration HTTP ajouté dans `v2/crates/sel-api/tests/postgres_admin_directory.rs`. | Exécution du test PostgreSQL attend la CI d’intégration ; aucune base de production n’a été interrogée. |
| Révocation appareil et autorisation hors ligne | IN_PROGRESS — UI et allowlist implémentées | La console d’administration permet la révocation explicite d’un appareil et d’un grant, après confirmation, puis recharge l’état central. Le pont natif n’autorise que `POST` avec UUID strict sur ces routes. | CI du nouveau code en attente. Aucun appareil ni grant de production n’a été modifié. |
| Vérification locale légère | PASS | `python -m pytest tests/test_v2_tauri_client.py -q` : **47 passed** ; `git diff --check` : PASS. Exécution SQLite in-memory du SQL des diagnostics : PASS (preuve locale de la tranche précédente). | Aucun `cargo test`, build React, compilation Tauri ou paquet Windows local exécuté afin d’éviter la surchauffe matérielle déjà signalée ; ces contrôles sont délégués à la CI. |
| Parcours de paiement | PARTIAL | Le client propose une créance réelle retournée par le relevé local, permet un versement partiel et affecte l’opération à une seule créance avec validation financière Rust. | L’API métier accepte plusieurs affectations, mais le formulaire n’en permet qu’une. Reçus et documents imprimables appartiennent à la phase 8. |
| Recette Windows/SEL | BLOCKED_EXTERNAL | Aucune recette sur poste représentatif, avec compte SEL, imprimante ou configuration serveur réelle n’a été réalisée dans cette tranche. | Requiert un environnement et une acceptation métier SEL ; ne peut pas être remplacée par des captures synthétiques CI. |

Les vérifications automatiques de ce commit sont encore `IN_PROGRESS` jusqu’à la fin des workflows déclenchés par son SHA exact. Le dossier utilisateur non suivi `artifacts/` est préservé et exclu des changements livrés.

**Progression Phase 7 : 97 % d’implémentation locale ; statut `IN_PROGRESS`.** La liste consultable des incidents, l’audit central en lecture seule et les actions de révocation sont codés et disposent de tests/contrats. Le pourcentage ne signifie pas validation CI du présent commit, recette Windows sur matériel réel, acceptation SEL ni déploiement. Les affectations de paiement multiples restent une lacune d’interface ; les PDF/reçus relèvent de la phase 8.

### Phase 7 — affectations de paiement multiples dans le client (4 novembre 2026)

Le formulaire d’encaissement prend maintenant en charge plusieurs créances distinctes au sein d’un même règlement. Chaque ligne est sélectionnée depuis le relevé réel du client et reçoit son montant ; les créances solaires exigent toujours une échéance ouverte. Le client bloque les doublons de créance et les mélanges de devise. L’API locale/Rust reste responsable de la validation atomique des montants, soldes, devise et échéance. Le scénario visuel synthétique couvre une créance ordinaire et une échéance solaire dans le même paiement ; il ne constitue pas un encaissement réel.

| Vérification | Statut | Preuve / limite |
|---|---|---|
| Formulaire multi-affectation et contrôles de saisie | PASS — code et CI | `v2/windows-app/src/App.tsx`, CSS dans `v2/windows-app/src/App.css`; requête envoyée avec la liste d’affectations `allocations`. Contrôle des doublons et des devises; échéance ouverte requise pour chaque plan solaire. |
| Captures et parcours visuel multi-affectation | PASS — CI Windows | `v2/windows-app/scripts/visual_review.py` construit le scénario créance ordinaire + échéance solaire et capture `payments-solar-1440x900.png`; workflow [Tauri Windows 41901575949](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41901575949) réussi. |
| Tests client ciblés | PASS local | `python -m pytest tests/test_v2_tauri_client.py -q` : **48 passed**. `python -m py_compile` des scripts de revue/tests : PASS ; `git diff --check` : PASS. |
| CI du code livré | PASS — cinq workflows sur le même SHA | Commit `a4ac91b1c99cbecc31f3c65ab59398d1e25dc7a9` : backend [41901576152](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41901576152), client Tauri Windows [41901575949](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41901575949), paquet serveur [41901575898](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41901575898), candidat Windows [41901575894](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41901575894), ancien client Windows [41901575952](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41901575952) : `success`. |
| Artefacts de validation | PASS build / NON RECETTÉS | Tauri `SEL-V2-Tauri-Windows-62`, 13 068 173 octets, SHA-256 `2198add57cea458fa38bc11ed11ea12c1e84911d29ee113cb9427156677944e7`; captures `SEL-V2-Figma-Desktop-Review-62`, 1 894 920 octets, SHA-256 `01d17679e877a8b0f2e14fccf249544a805c3b9f179dfe7a6e06831567e5acaa`; candidat `SEL-Facturation-V2-Windows-133`, 65 104 687 octets, SHA-256 `ba6d9b3beaad61a82d20cc59308de4076124288449c5fcf091c90db8748741ac`. Artefacts CI non installés et non acceptés par SEL. |
| Recette de paiement sur poste SEL | BLOCKED_EXTERNAL | Aucun paiement n’a été créé sur les données ou comptes SEL. Le scénario visuel CI est fictif ; une recette opérateur reste nécessaire. |

**Progression Phase 7 : 98 % d’implémentation vérifiée par tests ciblés et CI Windows ; statut `IN_PROGRESS`.** Les écrans et le parcours multi-affectation sont maintenant passés dans la CI Windows. Le pourcentage restant correspond aux parcours d’interface encore à compléter, à la recette Windows/SEL réelle (installation, DPI, compte et validation métier) et à la distinction entre artefact compilé et produit accepté. Les reçus/PDF relèvent de la phase 8. Cette CI ne prouve ni installation sur le poste SEL, ni déploiement, ni acceptation financière.

### Phase 8 — documents locaux comparés aux gabarits Figma (5 novembre 2026)

La génération locale des documents a été raccordée aux données structurées de l’agent Rust et mise en forme selon les gabarits A4 consultés dans Figma : facture combinée acceptée, facture provisoire, vente solaire, reçu de paiement et facture combinée avec eau positive. Le logo officiel SEL du fichier Figma est embarqué dans les ressources locales ; les PDF sont créés et prévisualisés sur le poste client, jamais enregistrés sur le serveur.

| Domaine | État | Preuve exécutée | Limite / décision restante |
|---|---|---|---|
| Structure des gabarits Figma | PASS — revue visuelle locale | Les compositions reprennent l’en-tête SEL, le titre et l’état, la référence, les blocs destinataire/document, le tableau de prestations, la synthèse, le QR interne et le pied de page. Les écrans source consultés sont les frames `8:2`, `8:65`, `8:128`, `8:184` et `53:277` du fichier SEL V2. | Revue de structure et d’aspect, sans test automatique de différence pixel à pixel. La validation finale du rendu imprimé appartient à SEL. |
| Facture combinée à 0 m³ | PASS — PDF rendu | `artifacts/phase8-documents-figma/invoice-pdf-lower-zero-water.png` montre les lignes électricité et eau à 0 m³, les montants, le total, le QR interne et le pied de page. Le canevas est validé comme un seul rendu A4. | Contenu commercial de test synthétique ; aucun tarif, profil fiscal ni identité d’entreprise réelle n’est approuvé par cette preuve. |
| Facture combinée à 5 m³ | PASS — PDF rendu distinct | `artifacts/phase8-documents-figma/invoice-pdf-lower-water-positive.png` montre 5 m³, un tarif de fixture de 1,30 USD/m³, la base, la taxe et le total. Le test calcule une empreinte du canevas et échoue si les sorties 0 m³ et 5 m³ sont identiques. | Le tarif et la fiscalité de la fixture ne constituent pas une approbation métier SEL. |
| Reçu et vente solaire | PASS — aperçus PDF | `payment-receipt-pdf-preview-1440x900.png` et `solar-pdf-preview-1440x900.png` ; le script vérifie un canevas visible, de dimensions A4 minimales, et exactement une page pour chacune des fixtures. | L’impression physique et l’enregistrement via le dialogue Windows ne sont pas exercés par ce test visuel. |
| Logo et fonctionnement local | PASS — bundle React | Le SVG clair fourni par Figma est importé depuis `v2/windows-app/src/assets/sel-logo-light.svg`; PDFMake et PDF.js sont intégrés localement et le build ne dépend pas d’un CDN pour ces ressources. | Le build Tauri/installateur et l’ouverture dans la fenêtre WebView installée n’ont pas été exécutés dans cette tranche. |
| Identité de l’émetteur et données légales | PARTIAL — données configurées seulement | Le document utilise le snapshot d’identité disponible et n’invente pas les adresses légales ou numéros RCCM/NIF absents. Les mentions d’exemple présentes dans Figma ne sont pas traitées comme des données SEL réelles. | SEL doit renseigner et valider ses valeurs dans la configuration pour obtenir le contenu final. |
| Vérifications locales | PASS | `npm run build` réussi ; `python -m py_compile scripts/visual_review.py` réussi ; `python scripts/visual_review.py` réussi avec captures à 1440×900 ; `cargo fmt --manifest-path v2/Cargo.toml --all -- --check` réussi ; `cargo test --manifest-path v2/Cargo.toml -p sel-local --test local_http` : **10/10** ; `cargo test --manifest-path v2/Cargo.toml -p sel-local --test local_commercial` : **2/2** ; `git diff --check` réussi (avertissements Git de conversion LF/CRLF uniquement). | Les tests PostgreSQL avec une instance réelle, l’impression, le lancement natif après installation et les parcours sur poste SEL ne sont pas validés ici. |
| Git / livraison | IN_PROGRESS | Les modifications sont présentes dans la branche de travail et le rapport demeure traçable ; les preuves visuelles locales sont dans `artifacts/phase8-documents-figma/`. | Aucun commit/push, installateur candidat ou résultat CI du nouvel état de travail n’est revendiqué ici. Le dossier non suivi `artifacts/` existant a été préservé. |

Les fixtures de cette revue sont synthétiques : elles servent à comparer la composition des documents et les cas 0 m³ / 5 m³. Le calcul commercial demeure effectué par Rust, et les snapshots des factures historiques ne sont pas recalculés. Le QR est présenté comme une référence interne SEL et ne prétend à aucune authentification fiscale.

**Progression Phase 7 : 98 % — `IN_PROGRESS`. Progression Phase 8 : 82 % — `IN_PROGRESS`.** La génération et le rendu visuel des principaux types de documents sont maintenant vérifiés sur des PDF locaux d’une page et les deux états d’eau sont visuellement distincts. Restent notamment la recette dans la WebView Tauri installée, le test réel de sauvegarde et d’impression Windows, la validation SEL des paramètres et mentions, l’intégration continue du commit final et les preuves de poste cible. Ces pourcentages décrivent l’implémentation vérifiée dans le dépôt, pas une acceptation commerciale ou une mise en production.

### Phase 7 — recette de l’application native sur le poste de développement (5 novembre 2026)

| Domaine | État | Preuve exécutée | Limite |
|---|---|---|---|
| Compilation native Windows x64 | PASS | `npm run tauri -- build --no-bundle` a terminé en profil Release ; exécutable : `v2/windows-app/src-tauri/target/release/sel-v2-windows.exe`. SHA-256 : `C1EDAAA96054247907D16B96D569DBA9F98A7D9FA36741F34C87A6F3C93C08ED`. La compilation a nécessité d’aligner `@tauri-apps/plugin-fs` sur la version native 2.7.1. | Binaire local non empaqueté ; aucun MSI/NSIS ni installation propre n’a été testé dans cette séance. |
| Démarrage et agent local | PASS limité | Le binaire a démarré dans une fenêtre Windows native. L’agent `sel-local.exe` embarqué a été lancé, puis arrêté à la fermeture de la recette. Les chemins `LOCALAPPDATA`, `APPDATA`, `USERPROFILE`, `TEMP`, `TMP` et la configuration appareil pointaient vers un profil temporaire isolé. | Il s’agit d’un profil de test, pas d’un poste SEL installé ou enrôlé. |
| Vérification visuelle des parcours | PASS limité | Tableau de bord, abonnés, relevés, factures, encaissements, kits solaires, créances, synchronisation et administration examinés dans la fenêtre 1440×900. États vides explicites, contrôles de saisie et indication des opérations en attente observés ; aucun débordement horizontal repéré. | Non vérifiés : matrice DPI/résolutions, lecteur d’écran, périphériques d’impression et session centrale réelle. |
| CRUD local d’un abonné | PASS | Création depuis l’interface avec les données synthétiques `SEL-QA-0001` / « RECETTE PHASE 7 — CLIENT TEMPORAIRE », puis modification du téléphone. La fiche a affiché la révision locale 2 et le bandeau 2 opérations en attente. Lecture SQLite en mode lecture seule : 1 abonné de test, révision 2, outbox `PENDING` = 2 ; factures, paiements, créances et ventes = 0. | Profil et données uniquement temporaires ; aucune synchronisation n’a été lancée. |
| Données existantes V1/V2 | BLOCKED par revue automatique | L’inventaire ciblé a confirmé que `C:\Users\MonarQue\AppData\Local\SEL Facturation` contient toujours 24 fichiers / 2 dossiers (~50 922 889 octets) et `C:\Users\MonarQue\AppData\Local\SEL Facturation V2` contient toujours 2 fichiers (~368 966 octets). | La suppression récursive demandée et autorisée par l’utilisateur a été rejetée par le contrôle automatique. Aucune donnée n’a été supprimée et aucun contournement n’a été tenté. |
| Fermeture / nettoyage du processus | PASS | La fenêtre native a été fermée ; le processus enfant restant a été identifié par son chemin exact dans le workspace puis arrêté. | Le profil temporaire de recette est conservé pour préserver les éléments de vérification. |

**Progression Phase 7 réévaluée : 99 % — `IN_PROGRESS`.** Le binaire Release fonctionne dans une fenêtre native sur ce PC et les parcours locaux consultés, ainsi que la création/modification SQLite d’un abonné synthétique, ont été exercés. Le 100 % n’est pas déclaré : l’installateur n’a pas été installé, l’identité/appareil ne sont pas enrôlés sur le serveur SEL, et l’authentification centrale, la synchronisation réelle, le DPI et l’impression restent non vérifiés. La recette ne touche pas aux données réelles. Aucun travail de phase 8 supplémentaire n’a été effectué pendant cette recette.

### Phase 8 — validation des documents et correction de l’impression (5 novembre 2026)

La phase 8 est maintenant achevée pour son périmètre de code et de vérification automatisée : documents construits depuis les données `document-data`/snapshots, contenu historique immuable, référence QR explicitement interne, pagination et mise en page A4 contrôlées sur les PDF produits par l’application. Le contrôle d’impression navigateur reproduit la feuille de style d’impression utilisée par la WebView, sans prétendre remplacer l’essai sur une imprimante physique.

| Vérification | État | Preuve exécutée / limite |
|---|---|---|
| Facture eau + électricité, eau à 0 m³ | PASS | PDF généré par l’application, 1 page A4 ; lignes et total extraits et vérifiés. Capture : `artifacts/phase8-documents-figma/invoice-pdf-lower-zero-water.png`. |
| Facture eau + électricité, eau à 5 m³ | PASS | PDF généré par l’application, 1 page A4 ; montant attendu par la fixture, base/taxe/total présents ; rendu distinct du cas 0 m³. Capture : `artifacts/phase8-documents-figma/invoice-pdf-lower-water-positive.png`. Les valeurs sont synthétiques et ne valident pas un tarif SEL. |
| Facture solaire et reçu | PASS | PDF A4 d’une page chacun ; texte métier, devise et montants extraits. Captures : `solar-pdf-preview-1440x900.png`, `payment-receipt-pdf-preview-1440x900.png`. |
| Contrat solaire de 36 échéances | PASS | PDF sur 2 pages A4 ; contrôle de la date localisée, des dernières échéances, de la synthèse financière et du QR interne conservés ensemble sur la dernière page. Captures : `solar-pdf-multipage-1440x900.png` et `solar-pdf-multipage-last-page-1440x900.png`. |
| Polices | PASS | Le script inspecte désormais chaque page des vrais PDF générés et exige un programme de police embarqué (`FontFile`, `FontFile2` ou `FontFile3`), y compris pour les polices Type0/descendantes. |
| Disposition d’impression | PASS — rendu navigateur | Le défaut qui créait 3 pages d’impression pour une facture d’une page est corrigé : les racines non documentaires sont retirées du flux d’impression et aucune page vide n’est ajoutée après le dernier canevas. Les factures 0/5 m³, le reçu et la vente solaire impriment sur le nombre A4 attendu ; le contrat long donne 2 pages. Ce contrôle Chromium n’est pas un essai d’imprimante physique. |
| Historique immuable | PASS — test de route existant | `v2/crates/sel-local/tests/local_http.rs` émet une facture, renomme le client après émission, puis relit `/document-data` et confirme que le snapshot réimprimé conserve le nom légal original. Le test ne modifie aucune base SEL réelle. |
| Revue visuelle des gabarits Figma | PASS — structure / NON pixel-perfect | En-tête, informations client, lignes des deux services, totaux, référence interne et pied de page sont contrôlés visuellement sur captures 1440×900 et comparés aux frames consignées plus haut. Aucune comparaison pixel par pixel ni validation par SEL n’est revendiquée. |
| Commandes exécutées | PASS | `npm run build` : réussi (avertissement de chunks volumineux uniquement). `python scripts/visual_review.py` : réussi après ajout du contrôle des polices embarquées sur chaque page. `git diff --check` : réussi ; avertissements de conversion LF/CRLF uniquement. Le test Rust d’immutabilité figure dans la suite locale ciblée déjà exécutée et documentée plus haut ; il n’a pas été relancé dans cette vérification finale. |
| Livrables et acceptation externe | BLOCKED_EXTERNAL | Captures conservées dans `artifacts/phase8-documents-figma/`. Les PDF de recette sont générés en mémoire et ne sont pas enregistrés sur le serveur. L’impression sur périphérique Windows physique, l’enregistrement via le dialogue Windows et l’acceptation SEL des données/mentions ne sont pas vérifiés. Les valeurs de fixtures ne sont pas des paramètres commerciaux approuvés. |
| CI et Git | NON EXÉCUTÉ | Aucun commit, push ou workflow CI n’a été lancé dans cette tranche ; le dépôt comporte des modifications préexistantes non commitées qui ont été préservées. |

Le défaut d’impression a été reproduit avant correction (3 pages au lieu d’une), corrigé dans `v2/windows-app/src/App.css`, puis le scénario complet a repassé. Le contrat long a aussi été ajusté dans `v2/windows-app/src/documents.ts` pour conserver les lignes du tableau et placer la synthèse/QR avec les dernières échéances. Le contrôle renforcé de `v2/windows-app/scripts/visual_review.py` vérifie les PDF binaires en mémoire, leurs pages A4, textes, QR interne et polices incorporées. Aucun PDF de facture n’est écrit sur le serveur.

**Progression Phase 8 : 100 % du périmètre technique et automatisé défini pour cette phase — `COMPLETED`.** Les critères d’émission locale, d’historique, d’identification interne du QR, de pagination A4, de polices embarquées et de rendu d’impression simulé sont passés. **Ce 100 % n’est pas une recette d’exploitation** : l’essai sur imprimante physique, l’enregistrement Windows, l’approbation métier des mentions et tarifs et la CI du nouvel état restent `BLOCKED_EXTERNAL` ou non exécutés. La phase 9 et les suivantes n’ont pas été commencées.

### Phases 9 et 10 — migration, exploitation et recette contrôlée (5 novembre 2026)

Le propriétaire confirme qu’aucune donnée réelle SEL n’est en exploitation. Les essais ajoutés utilisent donc exclusivement une base PostgreSQL jetable, un manifeste JSON synthétique et un profil Windows CI éphémère. Cette confirmation ne supprime pas l’obligation de tester la restauration et la migration avant un futur usage réel. Aucun fichier local SEL n’a été effacé, aucune base réelle n’a été migrée et aucun serveur n’a été déployé.

| Critère | Statut | Preuve / limite constatée |
|---|---|---|
| Import V1 en mode simulation | PASS local | `migration_dry_run` refuse maintenant un manifeste comportant un paiement explicitement dans une devise différente de sa facture (`BLOCKED_CURRENCY_MISMATCH`, zéro écriture). Le cas a été exécuté dans `migration_dry_run_blocks_payment_currency_mismatch`. Les références manquantes et identifiants dupliqués restent bloquants. |
| Import transactionnel et rejouabilité | PASS CI | `migration_apply` valide la devise dans la transaction et le rejeu reste idempotent. Le test PostgreSQL d’import et rapprochement s’est exécuté dans le job backend du commit `ab2df6686bbb88004ef9c5bff3937f9fbfcbaf09` : [run 41938832714](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41938832714), succès. |
| Rapprochement financier | PASS CI | `migration_reconcile` distingue les devises, rapproche les totaux des factures, paiements et soldes ouverts, et vérifie les factures/créances manquantes et devises incompatibles. Le test PostgreSQL a couvert une facture émise de 11,60 USD, un paiement de 5,00 USD et une facture annulée de 3,48 USD : total historique 15,08 USD, solde dû 6,60 USD. Job backend PostgreSQL : [run 41938832714](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41938832714), succès. |
| Sauvegarde/restauration PostgreSQL | PASS CI synthétique | Le test round-trip réel a utilisé une base UUID jetable : `pg_dump`, modification d’une ligne témoin, `pg_restore`, vérification de la restauration, suppression de la base isolée. Exécuté par le job backend avec PostgreSQL et client tools v16 : [run 41938832714](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41938832714), succès. Aucun système de production n’a été restauré. |
| Garde-fou de restauration | PASS local | La commande de restauration refuse par défaut et exige `SEL_RESTORE_APPROVED=I_UNDERSTAND`; le test de refus sans approbation passe. Aucun restore vers une base SEL ou de production n’a eu lieu. |
| Séparation configuration test/production | PARTIAL | `v2/deploy/validate-production.ps1` et Compose ont été vérifiés syntaxiquement avec des valeurs de test synthétiques; aucune valeur de secret n’a été fournie ni enregistrée. Ce contrôle de syntaxe n’établit pas la disponibilité TLS, les permissions effectives PostgreSQL, les sauvegardes externes ni l’isolation réseau du serveur réel. |
| Installation Windows utilisateur | PASS CI isolée | Le bundle NSIS fixe `installMode: currentUser`. Le workflow construit les setups Windows précédent et courant et les installe dans un profil CI synthétique `%LOCALAPPDATA%\cd.sel.facturation.desktop` : [run 41938832659](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41938832659), succès. |
| Mise à niveau et réparation/réinstallation | PASS CI isolée | Le smoke test exécute l’installation précédente, la mise à niveau, puis la réinstallation/réparation du setup courant et compare les empreintes de SQLite, identité appareil, outbox et document synthétique après chaque étape. Étape `Verify user install and upgrade preserve profile data` réussie, [run 41938832659](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41938832659). Aucun installateur n’a été lancé dans le profil utilisateur de ce PC ni sur un poste SEL. |
| Sauvegardes de données SEL existantes | NOT_REQUIRED_FOR_THIS_TEST | SEL indique qu’aucune donnée réelle n’existe encore. Les fixtures utilisées sont synthétiques. Une sauvegarde contrôlée demeure obligatoire avant toute future migration ou opération sur une base réelle, même si l’inventaire métier est alors vide. |
| Hôtes API et mises à jour | CONFIGURED_NOT_DEPLOYED | Le Caddyfile route `sel.shuleops.com` vers `sel-api` et `update.sel.shuleops.com` vers un répertoire statique en lecture seule; Compose conserve PostgreSQL/API sur le réseau interne et n’expose que Caddy. Les DNS A ont répondu `187.77.175.51` pour les deux hôtes. Le contrôle HTTPS actuel retourne 404 sur `/health/ready` côté API et présente un certificat dont le nom ne correspond pas à `update.sel.shuleops.com`; la configuration n’est donc pas encore déployée/servie comme SEL V2. La nouvelle valeur SafeUpdate V1 par défaut est `https://update.sel.shuleops.com/updates/latest.json`. |
| Retrait de V1 | BLOCKED — critère de sortie | Le plan exige couverture V2 et validation de l’export/migration avant retrait. Les installations et données locales historiques n’ont pas été supprimées et aucun composant V1 n’est retiré dans cette étape. L’affirmation qu’il n’existe pas encore de données réelles ne valide pas à elle seule l’export, la compatibilité ni le basculement. |
| Environnement de production et déploiement | BLOCKED_EXTERNAL | Les sous-domaines existent et résolvent, mais le routage applicatif, le certificat update, l’accès au serveur, les secrets de service, l’identité d’organisation et les clés de production restent non vérifiés/non provisionnés. Aucun déploiement n’a été tenté. PostgreSQL ne doit pas être exposé directement aux postes. |
| Retours locaux | PASS | `cargo fmt --manifest-path v2/Cargo.toml --all -- --check`; `cargo test --manifest-path v2/Cargo.toml --workspace` (tous les tests exécutables réussis; intégrations PostgreSQL exécutées par CI); `cargo clippy --manifest-path v2/Cargo.toml --workspace --all-targets -- -D warnings`; `python -m pytest -q tests/test_v2_tauri_client.py` (**49 passed**); `python -m py_compile` des scripts de revue/installateur; `git diff --check`. |
| CI du candidat précédent | PASS | Sur le HEAD applicatif `ab2df6686bbb88004ef9c5bff3937f9fbfcbaf09`, les cinq workflows sont verts : backend PostgreSQL [41938832714](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41938832714), Tauri/Windows et cycle install/upgrade [41938832659](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41938832659), Windows candidate [41938832665](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41938832665), paquet serveur [41938832677](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41938832677), Windows desktop historique [41938832701](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41938832701). |
| CI de la configuration des hôtes | PASS | Le workflow `SEL V2 server package` a exécuté `docker compose config --quiet`, `caddy validate --config ... --adapter caddyfile` et la construction de l’image serveur sur le commit `6c81338317fbe95053594f318e7814b44e717520` : [run 41965094902](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41965094902), succès. |
| CI de la configuration finale | PASS | Sur le commit `6c81338317fbe95053594f318e7814b44e717520`, les cinq workflows sont verts : backend Rust [41965094927](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41965094927), Tauri/Windows [41965094894](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41965094894), Windows candidate [41965094898](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41965094898), paquet serveur avec validation Caddy [41965094902](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41965094902), Windows desktop historique [41965094893](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41965094893). |

Les workflows utilisent un runner Windows éphémère et des marqueurs synthétiques; le test refuse d’écraser un profil existant. Le job backend s’appuie sur PostgreSQL réel de CI et les outils clients v16. Les installateurs générés sont des artefacts CI non signés, attachés au run `41965094894`; ils prouvent l’assemblage du commit de configuration, mais ne constituent pas un installateur accepté par SEL ni une recette matérielle. La configuration des hôtes API/updates n’a pas été appliquée au serveur; Caddy doit encore être déployé et obtenir les certificats, et le canal restera 404 tant qu’aucun manifeste signé n’est publié. Aucune vérification sur poste SEL, imprimante physique ou paramètres commerciaux n’a été faite.

**Progression Phase 9 : 80 % — `IN_PROGRESS`. Progression Phase 10 : 70 % — `IN_PROGRESS`. Progression combinée phases 9–10 : 75 % — `IN_PROGRESS`.** Les migrations, rapprochements, sauvegarde/restauration synthétique, fabrication des setups, installation utilisateur, mise à niveau et réinstallation ont des validations automatisées réussies. Les hôtes demandés sont intégrés à la configuration; leur validation syntaxique et les cinq workflows du commit de configuration sont verts. Restent le déploiement serveur contrôlé, les certificats et secrets de production, l’intégration du transport LiveUpdate V2, le retrait de V1, les critères commerciaux et la recette matérielle/SEL. Aucun verdict `GO_PRODUCTION` n’est émis.

### Addendum — déploiement contrôlé et candidat Windows (5 novembre 2026)

Cet addendum remplace les lignes précédentes qui indiquaient qu’aucun déploiement n’avait été tenté. Le backend a depuis été démarré sur le serveur Hostinger `kit-service`, en cohabitation avec les services existants, sans les modifier.

| Critère | Statut | Preuve / limite |
|---|---|---|
| API centrale HTTPS | PASS — disponibilité | `https://sel.shuleops.com/health/ready` renvoie HTTP 200 avec certificat valide. Une requête `GET /v1/customers?limit=1` sans session renvoie HTTP 401. |
| Isolation PostgreSQL | PASS — configuration observée | PostgreSQL SEL est dans le réseau Docker interne, sans port hôte publié. L’API est sur l’adresse privée stable `172.20.0.10:8080`; Nginx la relaie sur `/v1/` et `/health/`. |
| Démarrage serveur | PASS — API prête | Image API construite depuis le code applicatif du commit `1389d9c1c693d47d935376a326bd6e8aa43e67d4`; configuration Compose corrigée sur `ca4be87021a531edd8d23c15817c04e868708445`. PostgreSQL et migrations démarrent, endpoint readiness sain. |
| Initialisation du rôle PostgreSQL | CORRIGÉ / PASS CI | Le premier lancement a révélé que le rôle applicatif n’était pas transmis au conteneur PostgreSQL. Le modèle Compose et le workflow ont été corrigés dans `ca4be87`; le workflow serveur [41972463013](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41972463013) a réussi avec test d’intégration de démarrage PostgreSQL/API. |
| État des données SEL | PASS — base neuve | Organisation et compte ADMIN initial créés. Aucun client, facture ou paiement métier n’est présent. Les autres volumes et services du serveur n’ont pas été touchés. Aucun PDF de facture n’est stocké côté serveur. |
| TLS update | PASS — certificat / PARTIAL — service | `https://update.sel.shuleops.com` présente maintenant un certificat valide. Le manifeste `/updates/latest.json` renvoie 404 car aucun manifeste LiveUpdate signé n’a été publié; le transport LiveUpdate du client V2 n’est pas encore opérationnel. |
| Chemins V1 existants | PASS — préservés | Les emplacements `/stable/` et `/pilot/` et les fichiers présents avant intervention ont été conservés. La configuration Nginx initiale a une copie de retour arrière sous `/root/sel-v2-nginx-backup-20261105T052625Z`. |
| Release Windows 0.4.0 | PASS — préversion publiée | [Release `v0.4.0`](https://github.com/jupi-z/SEL-Facturation-Desktop/releases/tag/v0.4.0), `prerelease=true`, artefact construit par CI sur le commit `1389d9c`. EXE et MSI non signés Authenticode; `SHA256SUMS.txt` joint. Le cycle d’installation/mise à niveau a réussi sur runner Windows CI, sans installation physique SEL dans cette intervention. |
| Recette commerciale complète | BLOCKED_EXTERNAL | Tarifs/référentiels de production, autorisations hors ligne, synchronisation multi-postes réelle, impression sur périphérique SEL et installation V2 sur poste représentatif restent à vérifier. La Release est un candidat de recette, pas une livraison commerciale. |

**Verdict actualisé : `GO_CANDIDATE`, pas `GO_PRODUCTION`.** L’API est déployée et joignable; le candidat Windows est disponible. Les opérations hors ligne et la recette commerciale complète ne sont pas démontrées, et aucun manifeste de mise à jour V2 n’est activé.

### Recontrôle de fonctionnement des opérations essentielles (5 novembre 2026)

Ce contrôle a séparé les vérifications de production en lecture seule des tests exécutés sur données synthétiques. Aucun client, facture, paiement, tarif ou appareil n’a été créé dans la base SEL pendant cette vérification.

| Critère | Statut | Preuve / limite |
|---|---|---|
| Santé de l’API production | PASS — lecture seule | `GET https://sel.shuleops.com/health/ready` renvoie HTTP 200; `GET /v1/customers?limit=1` sans session renvoie HTTP 401. |
| Connexion et schéma PostgreSQL production | PASS — lecture seule | Connexion au conteneur PostgreSQL sous `default_transaction_read_only=on`; base `sel_v2`, 15 migrations SQLx appliquées, tables métier attendues présentes. |
| Données et référentiels production | BLOCKED — poste non prêt | Compteurs lus : 0 client, 0 facture, 0 paiement, 0 vente, 1 utilisateur, 0 appareil, 0 grant hors ligne, 0 tarif, 0 profil fiscal, 0 produit, 0 taux de change et 0 compte de service. Aucun calcul de facture commerciale réelle ne peut être validé sans référentiels approuvés. |
| EXE Windows installé sur ce poste | BLOCKED — activation absente | `sel-v2-windows.exe` est en version 0.4.0 et `sel-local.exe` est présent; le fichier `%LOCALAPPDATA%\\cd.sel.facturation.desktop\\device.json` est absent. Tauri ne démarre donc pas l’agent local et les routes métier ne peuvent pas aboutir. |
| Format Rust | PASS local | `cargo fmt --all -- --check`. |
| Analyse statique Rust | PASS local | `cargo clippy --workspace --all-targets -- -D warnings`. |
| Tests Rust disponibles localement | PASS local, PostgreSQL ignoré | `cargo test --workspace` termine avec succès; les tests annotés `ignored` requièrent `TEST_DATABASE_URL`, absent localement. |
| Tests PostgreSQL d’intégration | PASS CI sur commit de référence | Le workflow backend du commit `d35eb8c809defad400f072025f0555de2d8573ad` exécute `cargo test --workspace -- --ignored` avec PostgreSQL 16.4 isolé : [run 41974474070](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41974474070), succès. Les données de ce service CI sont synthétiques. |
| Tests du client Windows | PASS local | `py -3.12 -m pytest -q tests/test_v2_tauri_client.py` : 50 tests réussis. |
| Compilation frontend | PASS local | `npm run build` dans `v2/windows-app`; Vite et TypeScript réussissent. |
| Workflows Windows | PASS CI sur commit de référence | Build Tauri : [run 41974474099](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41974474099); candidat Windows et smoke d’installation : [run 41974474082](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41974474082), succès. Ces tests utilisent un profil synthétique, pas une installation SEL activée. |

**Verdict d’exploitation après ce recontrôle : `NO_GO` pour l’utilisation métier de l’EXE actuellement installé.** Le backend répond et les tests isolés du moteur passent, mais le poste réel n’est pas enrôlé et la base de production n’a aucun référentiel tarifaire. L’activation sécurisée, le provisionnement des référentiels approuvés puis une recette métier sur client de test restent obligatoires. Aucun EXE fonctionnel nouveau n’est déclaré ni publié par ce contrôle.

### Correctif d’accès initial et vérification production (5 novembre 2026)

Cette intervention corrige le premier parcours de connexion centrale sans configuration locale, et prépare la création d’un compte employé à mot de passe temporaire. Elle ne prétend pas rendre les opérations commerciales fonctionnelles tant que l’enrôlement du poste et les référentiels ne sont pas activés.

| Critère | Statut | Preuve / limite |
|---|---|---|
| Connexion centrale avant enrôlement local | PASS — code et test natif | Le lanceur utilise `https://sel.shuleops.com` par défaut, accepte un override HTTPS et refuse HTTP. Le test Tauri `production_api_url_is_available_before_device_enrollment` passe. |
| Mot de passe temporaire et première connexion | PARTIAL — code ajouté | Migration additive `0016_password_change_required.sql`, route `/v1/auth/change-password`, refus des routes métier tant que le mot de passe doit changer, invalidation des sessions et option activée par défaut dans l’administration. Un test PostgreSQL HTTP complet est ajouté, mais exige `TEST_DATABASE_URL` et n’a pas été exécuté localement. La migration n’a pas été appliquée en production et le compte Clarisse n’a pas été créé. |
| Tests Rust | PASS local (tests non ignorés) | `cargo fmt --all -- --check` et `cargo test --workspace` dans `v2/` passent. Les tests PostgreSQL sont explicitement ignorés localement en l’absence de `TEST_DATABASE_URL`; ils ne sont pas déclarés PASS local. |
| Tests natifs Windows | PASS local | `cargo test --manifest-path src-tauri/Cargo.toml --lib` : 3 tests réussis; `cargo test --manifest-path src-tauri/Cargo.toml --no-run` compile les exécutables de test. Aucun installateur n’a été construit ni installé sur le poste dans cette étape. |
| Analyse statique Rust | PASS local | `cargo clippy --workspace --all-targets -- -D warnings` dans `v2/` termine sans avertissement. |
| Client et interface | PASS local, avertissement de taille | `py -3.12 -m pytest -q tests/test_v2_tauri_client.py` : 51 tests réussis. `npm run build` réussit; Vite signale des chunks PDF/font dépassant 500 kB. |
| Disponibilité actuelle production | PASS réseau / FAIL configuration métier | Lecture HTTP du 5 novembre : `/health/live` = 200, `/health/ready` = 200 avec `configured:false`, et `update.sel.shuleops.com/updates/latest.json` = 404. L’API et PostgreSQL tournent sur l’hôte SSH `kit-service`, mais aucun tarif actif n’est chargé. Ces réponses ne démontrent pas une facturation possible. |
| Base de production modifiée | PASS — aucune écriture | Aucun compte, client, tarif, facture ou paiement n’a été créé; migration `0016` non déployée. |
| Enrôlement et travail hors ligne | BLOCKED — P0 | Sans `device.json`, l’agent SQLite ne démarre pas. L’interface n’a pas encore de parcours d’enrôlement/provisionnement, et le serveur ne dispose pas d’émetteur de signature Ed25519 permettant de délivrer un grant au poste. Le worker local reçoit actuellement un jeton central d’accès limité dans le temps, sans cycle de rafraîchissement persistant pour assurer la reprise automatique à long terme. |
| Mise en service commerciale | NO_GO | Les référentiels (tarifs, profils fiscaux, produits et taux approuvés) ne sont pas actifs; l’émission de factures ne peut donc pas être validée. Aucun compte Clarisse ni nouvel EXE de production n’est livré par cette étape. |

**Décision : `NO_GO` pour une livraison production.** Les correctifs de démarrage et de première connexion sont prêts dans la branche, mais une simple compilation ne résout ni le poste non enrôlé, ni la signature des autorisations hors ligne, ni l’absence de tarifs. Ne pas déployer la migration ou présenter un nouvel EXE comme exploitable avant validation PostgreSQL intégrée, parcours d’enrôlement complet, référentiels SEL approuvés et recette Windows réelle.

### Précision du tarif de l’eau communiquée par le propriétaire (5 novembre 2026)

Le propriétaire a confirmé que le tarif d’eau est **0,30 USD/m³**. Les exemples visuels et fixtures de facturation qui représentaient le tarif SEL à 1,30 USD/m³ ont été corrigés; une facture synthétique de 2,5 m³ vérifie un sous-total eau de 0,75 USD, une TVA de fixture de 0,12 USD et un total de ligne de 0,87 USD. Cette vérification arithmétique ne valide pas à elle seule l’assiette ou l’application fiscale de l’eau. Aucun tarif ni profil fiscal n’a été écrit dans PostgreSQL de production; le catalogue de production reste à provisionner après connexion/authentification et l’intégration de ce client.

### Activation de l’unique poste et candidat Windows 0.4.1 (5 novembre 2026)

Le propriétaire confirme qu’il n’y a qu’un seul poste et qu’aucun plafond de montant des encaissements hors connexion ne doit être imposé. L’autorisation reste signée par l’API et expire après 7 jours; cette durée permet une reconnexion maîtrisée et doit être renouvelée au retour en ligne. L’agent métier local est désormais rattaché à l’utilisateur actif sélectionné (Agent facturation ou Caissier), pas au compte super-administrateur.

| Critère | Statut | Preuve / limite |
|---|---|---|
| Route de provisionnement poste | IN_PROGRESS | `POST /v1/admin/devices/{id}/provision` signe le grant Ed25519 côté serveur, vérifie l’organisation/l’administrateur/l’utilisateur actif et persiste appareil, hash du jeton, grant, audit et idempotence en une transaction. Migration additive `0017_device_sync_tokens.sql`. |
| Activation depuis l’application | PASS — compilation et tests locaux | La console d’administration propose un utilisateur métier actif; le pont Tauri conserve un brouillon privé et idempotent, puis écrit le `device.json` et démarre l’agent local. Les tests du pont passent; aucune installation sur un poste SEL ni activation réelle n’est encore attestée. |
| Autorisation hors connexion | PASS — choix du propriétaire appliqué | Aucun plafond monétaire; portée des commandes incluse dans le grant signé; expiration de 7 jours. Les commandes restent marquées locales jusqu’à l’accusé central durable. |
| Compte Clarisse | BLOCKED — écriture centrale non effectuée | Le compte `clarisse.bomatsa@selsasu.com` n’a pas encore été créé. Il nécessite un appel authentifié après que le serveur déployé applique la migration `0017`; son mot de passe temporaire doit être remis par un canal sûr et changé au premier accès. |
| Validation locale | PASS partiel | `cargo fmt --all -- --check`, `cargo test --workspace` (tests non ignorés), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --lib` Tauri (3 tests), `python -m pytest tests/test_v2_tauri_client.py -q` (51 tests), `npm run build` réussissent. Le build React émet uniquement un avertissement de taille des ressources PDF/polices. |
| PostgreSQL intégré | BLOCKED_EXTERNAL — CI requise | Docker Desktop n’est pas démarré localement. Le test `postgres_device_provision` requiert `TEST_DATABASE_URL`; il vérifie signature, transaction, rattachement à l’opérateur et restriction du jeton aux routes de synchronisation. Aucun résultat PostgreSQL local n’est revendiqué. |
| Configuration de production | BLOCKED — clés et catalogue | L’API de production répond, mais le contrôle précédent indiquait `configured:false`, zéro tarif/profil fiscal approuvé et aucune paire de clé émettrice Ed25519. La paire de production doit être créée et protégée sur le serveur avant d’activer cette route; aucune clé privée ne doit être copiée dans Git ou le client. |
| Paramètres commerciaux | BLOCKED — approbation SEL nécessaire | Prix d’eau confirmé : 0,30 USD/m³. Barèmes électriques existants dans V1 : 0,095 résidentiel et 0,12 commercial USD/kWh; TVA 16 % historiquement configurée. Aucun barème/profile fiscal n’a été écrit en production. L’assiette fiscale de l’eau ne doit pas être déduite d’une fixture; les versions doivent passer par le workflow d’approbation et de date d’effet prévu. |
| Version binaire | IN_PROGRESS | Le paquet Tauri passe à 0.4.1. Aucun installateur final n’est encore produit, attaché à une CI verte pour ces modifications ou installé sur le poste SEL. |
| Déploiement production | NO_GO à cette étape | Aucun déploiement de ce candidat n’a été effectué. La clé privée Ed25519, la migration PostgreSQL, la CI avec vraie base de test, l’activation, la création du compte employé, les référentiels approuvés et une facture de test hors production restent à valider avant exploitation. |

### État de production et candidat Windows — 5 novembre 2026 (mise à jour)

| Critère | Statut | Preuve vérifiée |
|---|---|---|
| Branche / commit déployé | PASS | Branche `monarque/sel-v2-backend`, commit `67ae15fde0a5b53fef6ef35905c31fe012fc2519`. Le marqueur `/srv/sel-v2/DEPLOYMENT_COMMIT` correspond au commit. |
| Déploiement API et PostgreSQL | PASS — disponibilité | Conteneurs Compose `sel-v2-sel-api-1` et `sel-v2-postgres-1` en état healthy. Le digest de l'image API est `sha256:1b6cff370443ac459c0e183df47e96e79d0b363aaa654960fa75fbbff88f5214`. PostgreSQL n'expose aucun port hôte. |
| HTTPS / contrôle API | PASS — disponibilité et contrôle d'accès | `GET /health/live` et `GET /health/ready` sur `https://sel.shuleops.com` : HTTP 200. Les routes protégées `/v1/customers` et `/v1/admin/users` sans session répondent HTTP 401. `/health/ready` annonce `configured:false`. |
| Migration PostgreSQL | PASS — structure | 17 migrations SQLx appliquées au redémarrage du candidat. Aucun changement de données commerciales réelles n'a été fait. |
| Sauvegarde / restauration pré-déploiement | PASS — test isolé | Archive `/srv/sel-v2/backups/predeploy-20261105T083304Z.dump`, 82 648 octets, SHA-256 `e604760cb40fea5b4875bc04b6bd85e87583f5f86b7aa8e1e4c57559e985b994`; `pg_restore --list` et restauration dans une base temporaire isolée réussies, puis base temporaire supprimée. |
| Compte SEL Clarisse | PASS — création et connexion | `clarisse.bomatsa@selsasu.com` créé comme `BILLING_AGENT`. Connexion réelle vérifiée; `require_password_change=true`. Le mot de passe temporaire a été remis au propriétaire dans la conversation et n'est pas consigné ici. |
| Référentiels de facturation de production | BLOCKED — validation métier | Les routes de catalogue renvoient des listes vides et l'API reste `configured:false`. Le tarif eau de 0,30 USD/m³ est connu, de même que les paramètres V1 d'électricité et la TVA historique, mais l'assiette TVA de l'eau et les références d'approbation effectives ne sont pas confirmées ici. Aucun tarif ni profil fiscal n'a été créé ou approuvé en production. Aucune émission commerciale ne doit être faite avant leur configuration et leur approbation. |
| Manifeste LiveUpdate | BLOCKED — clés / transport | `https://update.sel.shuleops.com/updates/latest.json` répond HTTP 404. Aucun manifeste signé n'a été publié; ne pas traiter LiveUpdate V2 comme opérationnel. |
| CI backend PostgreSQL | PASS | Workflow [41994859285](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41994859285) réussi sur le commit déployé. |
| CI Tauri Windows | PASS | Workflow [41994859307](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/41994859307) réussi : tests client, captures synthétiques à la taille Figma, compilation agent, tests natifs, MSI/NSIS et installation/mise à niveau isolée préservant les données de profil. |
| Artefact Windows 0.4.1 | PASS — candidat CI, non signé | NSIS : `SEL Facturation_0.4.1_x64-setup.exe`, SHA-256 `FBAC1F07691CABA706E76DFC75EBE00741FFB8038443CBDC9B41E5296906A76C`. MSI : `SEL Facturation_0.4.1_x64_en-US.msi`, SHA-256 `D625B22F0235459B1B5A9B0B09525AA825F4EAA5E8B4FD53FFDF39291F815B47`. Vérification Authenticode : `NotSigned`; candidat de recette, pas version commerciale signée. |
| Installation sur poste SEL et recette commerciale | BLOCKED_EXTERNAL | La CI Windows utilise un poste virtuel et un profil synthétique. L'installation, l'activation de l'appareil, l'impression réelle, les factures avec eau positive et la synchronisation après coupure n'ont pas été validées sur le poste d'exploitation SEL. |

**Verdict : `NO_GO` pour la facturation de production; `GO_CANDIDATE` pour le candidat Windows et le backend déployé.** Le service central répond, l'accès est contrôlé et le compte métier existe. La configuration tarifaire/fiscale absente empêche les opérations commerciales; l'absence de manifeste signifie que les mises à jour V2 distantes ne sont pas actives. Ne pas présenter le système comme pleinement exploitable tant que ces blocages ne sont pas levés et la recette sur le poste cible effectuée.

### Blocage P0 détecté avant approbation des tarifs (5 novembre 2026)

L'examen préalable à toute écriture de tarifs en production a révélé une lacune fonctionnelle : `tariff_versions` enregistre `customer_category`, mais le modèle client V2 et le formulaire de création Windows ne portent pas cette catégorie. Les fonctions centrales et SQLite `load_tariffs` filtrent service, unité, devise, révision et date, mais pas catégorie; elles terminent par `LIMIT 1`. Avec plusieurs catégories, le prix choisi n'est donc pas déterministe ni garanti applicable à l'abonné. Aucune tarification n'a été chargée en production pour éviter un risque de montant incorrect.

| Critère | Statut | Suite indispensable |
|---|---|---|
| Catégorie portée par le dossier abonné | FAIL — lacune de modèle et d'interface | Ajouter une catégorie additive, préservant les dossiers existants; rendre les dossiers historiques explicitement non classés jusqu'à correction. Ajouter la saisie/édition et validation Windows. |
| Sélection du tarif par catégorie | FAIL — moteur central et local | Résoudre la catégorie depuis le dossier autoritatif (pas une valeur libre dans la facture), filtrer explicitement les tarifs dans PostgreSQL/SQLite et refuser toute catégorie sans tarif approuvé. Appliquer aussi cette règle à la synchronisation. |
| Régression financière | NOT_TESTED — correction non commencée | Ajouter des tests API et SQLite avec tarifs résidentiel et commercial simultanés, vérifier les prix corrects, les erreurs si catégorie ou tarif manquent, et la même résolution hors ligne/centrale. |
| Paramètres tarifaires en production | BLOCKED | Ne créer ni approuver aucun barème avant correction et tests. Le prix d'eau confirmé est 0,30 USD/m³; les valeurs historiques V1 sont documentées plus haut. La période d'effet et l'assiette TVA eau devront être consignées dans des références d'approbation vérifiables. |

Cette lacune renforce le verdict `NO_GO` pour la facturation commerciale. La version 0.4.1 reste un candidat d'installation technique; elle ne doit pas servir à émettre des factures avant cette correction, l'approbation du catalogue et une recette métier.

### Correction de sélection des tarifs par catégorie — candidat 0.4.2 (5 novembre 2026)

La lacune décrite ci-dessus est corrigée dans le code de la branche `monarque/sel-v2-backend`. Les clients portent désormais une catégorie facultative et normalisée, qui peut être créée ou modifiée dans l'application. La sélection centrale et SQLite exige la catégorie du dossier client et filtre les tarifs par catégorie, service, unité, devise, référence/version approuvée et période d'effet. Un client ancien sans catégorie, un tarif absent ou plusieurs tarifs applicables produisent maintenant une erreur explicite; l'émission et la synchronisation ne choisissent pas arbitrairement un tarif. La projection de synchronisation conserve et peut effacer explicitement la catégorie.

| Critère | Statut | Preuve / limite |
|---|---|---|
| Catégorie dans les dossiers client | PASS — code | Modèle et migrations additives (`0018_customer_category.sql` PostgreSQL, `0014_customer_category.sql` SQLite), API, projection de synchronisation et formulaire Windows mis à jour. Les dossiers historiques restent non classés jusqu'à leur mise à jour explicite. |
| Résolution tarifaire centrale et locale | PASS — tests | PostgreSQL et SQLite sélectionnent le tarif correspondant à la catégorie; l'absence de catégorie/tarif et les doublons applicables échouent de manière fermée. Test réel : `cargo test -p sel-storage --test category_tariff_selection -- --ignored postgres_tariffs_are_selected_for_the_customer_category` — PASS sur PostgreSQL 16.4 dans le conteneur de test `codex-sel-final-tests`. Test SQLite du même fichier — PASS. |
| Synchronisation de la catégorie | PASS — tests | Le test du worker local vérifie le transfert de catégorie et son effacement explicite; la suite PostgreSQL `two_sqlite_devices_converge_through_real_postgres_and_http` passe sur une base vierge. |
| Régressions PostgreSQL | PASS partiel local | `TEST_DATABASE_URL` vers une base PostgreSQL de test fraîche : les tests intégrés commerciaux, utilisateurs/RBAC, catalogue, appareils et grants, relevés, factures concurrentes, paiements concurrents, solaire, annulations, convergence multi-postes et synchronisation financière passent. `cargo test --workspace -- --ignored` exécuté avec `--skip migration_apply_is_transactional_and_idempotent_on_postgres --skip postgres_backup_restore_round_trip_uses_isolated_database` passe. Ces deux tests nécessitent les exécutables Windows `pg_dump` et `pg_restore`, absents de l'environnement local; ils restent actifs dans la CI. |
| Suite locale standard / analyse statique | PASS | Dans `v2/` : `cargo fmt --all -- --check`, `cargo test --workspace` et `cargo clippy --workspace --all-targets -- -D warnings` réussissent. Les tests PostgreSQL marqués `ignore` nécessitent la commande explicite avec `TEST_DATABASE_URL`, exécutée séparément ci-dessus. |
| Build du client Windows | PASS — build web | `npm run build` dans `v2/windows-app`, version 0.4.2 : PASS. Vite conserve un avertissement de taille pour les ressources PDF et polices; aucun échec de compilation. Le paquet MSI/NSIS de cette révision dépend encore de la CI Windows et n'est pas déclaré produit ni testé sur le poste SEL. |
| Fichiers/factures en production | PASS — aucune écriture | Aucun tarif, profil fiscal, utilisateur, facture ou paiement n'a été écrit en production pendant cette correction. Le serveur de production demeure sur le commit précédemment déployé jusqu'à revue de la CI et levée des blocages métier. |
| Paramètres et disponibilité commerciale de production | BLOCKED — P0 | L'API renvoie toujours `configured:false`; les référentiels de production sont vides. Le prix d'eau fourni par le propriétaire est 0,30 USD/m³, mais l'assiette/règle fiscale de l'eau, les références d'approbation et les périodes d'effet doivent être enregistrées sans extrapolation. Aucune facture réelle ne doit être émise avant leur configuration contrôlée. |
| Appareil et mises à jour | BLOCKED — production | L'enrôlement avec une clé Ed25519 de production doit être vérifié sur le poste cible. Le manifeste `https://update.sel.shuleops.com/updates/latest.json` répondait HTTP 404 au dernier contrôle; LiveUpdate n'est donc pas opérationnel. |
| CI distante / paquet d'installation | NOT_YET_VERIFIED | Les modifications locales ne sont pas encore attachées à une exécution CI. Le candidat Windows 0.4.2 n'est ni publié ni validé sur un poste SEL. |

**État du candidat :** correction de catégorie `PASS` localement et sur PostgreSQL de test; client web 0.4.2 compilé. **Verdict d'exploitation : `NO_GO` pour la facturation en production.** Les paramètres fiscaux approuvés et chargés, l'activation réelle de l'appareil, le manifeste de mise à jour et la recette Windows sur le poste SEL manquent toujours. Le déploiement du candidat et la publication d'un installateur ne sont pas effectués par cette vérification.

### Bascule du client Windows V2 en mode connecté uniquement (5 novembre 2026)

Le client Windows V2 est désormais modifié pour travailler exclusivement via l'API centrale configurée (`https://sel.shuleops.com` par défaut). Il ne lance plus l'agent SQLite, n'envoie plus de requêtes `/local/v1` ou `/v1/sync`, et le pont Tauri refuse aussi la provision d'appareils et la gestion de grants hors ligne. Une connexion utilisateur est requise au démarrage; le jeton de renouvellement est protégé par DPAPI, permet de conserver la session après redémarrage et ne repousse pas l'échéance absolue de trois jours pendant sa rotation. L'état « serveur joignable mais catalogue non configuré » est distingué d'une panne réseau : l'administrateur peut s'authentifier pour initialiser les référentiels, tandis que les parcours commerciaux restent bloqués. En cas de réelle indisponibilité centrale, l'interface masque les données métier et verrouille les opérations. Les clés d'idempotence de requête sont des marqueurs de reprise sans contenu métier; aucun payload métier n'est mis en file locale. La génération de PDF demeure locale à partir des données structurées renvoyées par l'API.

| Critère | Statut | Preuve / limite |
|---|---|---|
| Client métier exclusivement connecté | PASS — code et tests de pont | Le client n'exécute pas `sel-local`; les listes et écritures utilisent les endpoints `/v1`; le pont refuse les routes locales, de synchronisation offline, de provision d'appareil et de grants offline. Build TypeScript/Vite réussi et 4 tests natifs Tauri réussis. |
| Ventes solaires centrales | PASS — API et PostgreSQL de test | `GET /v1/solar/sales?limit=100` et `GET /v1/solar/sales/:id/document-data` ajoutés; intégrés au chargement Windows. Le scénario PostgreSQL de vente/contrat/échéancier et les nouvelles assertions HTTP de liste/document passent sur le conteneur de test. |
| Session utilisateur, redémarrage et durée maximale | PASS — implémentation et test PostgreSQL | DPAPI conserve uniquement le jeton de renouvellement; expiration serveur à trois jours; test de rotation confirme que l'échéance absolue ne glisse pas. Migration additive `0020_three_day_session_expiry.sql` limite aussi les sessions déjà ouvertes lors de son application. |
| Échec réseau et configuration métier incomplète | PASS — code | Les états réseau et catalogue sont séparés. L'authentification administrateur reste possible quand le serveur répond avec `configured:false`; l'interface ne donne accès qu'à l'initialisation, sans autoriser les opérations commerciales. En panne réseau, les écrans se verrouillent et les données métier en mémoire sont vidées. Les listes échouées sont vidées pour ne pas exposer des données périmées. |
| Build client et tests locaux | PASS partiel | `npm run build` PASS; `cargo fmt --all` PASS; tests Tauri 4/4 PASS; test PostgreSQL solaire 1/1 PASS; test PostgreSQL de rotation de session 1/1 PASS; tests `cargo test -p sel-api -p sel-storage` PASS pour les suites non ignorées. Les tests Postgres explicitement marqués `ignore` n'ont pas tous été lancés dans cette tranche. Le build Vite signale encore des bundles PDF/polices supérieurs à 500 kB. |
| API de production | BLOCKED — configuration métier | Vérification HTTP en lecture seule de `https://sel.shuleops.com/health/ready` : HTTP 200, `configured:false`. Les opérations métier restent correctement bloquées tant que le catalogue commercial actif n'est pas provisionné. Aucune écriture financière ni migration de production n'a été faite pendant cette tranche. |
| Authentification Figma | BLOCKED_EXTERNAL — fichier de design inaccessible | Le lien Figma fourni n'est pas accessible par l'outil de lecture disponible et aucun connecteur Figma n'est exposé dans cette session. Le formulaire présent n'est donc pas certifié comme copie fidèle des frames Figma; aucune conformité visuelle exacte n'est revendiquée. |
| Installation Windows / exploitation | NOT_TESTED | Aucun nouvel installateur n'a été produit ou installé pendant cette tranche; pas de recette de connexion avec un utilisateur réel sur le poste SEL, ni de validation imprimante. |

**Verdict de cette tranche :** le chemin réseau côté client est corrigé et vérifié jusqu'à un PostgreSQL local de test, mais le produit reste `NO_GO` pour l'exploitation commerciale tant que le serveur de production reste `configured:false`. La disponibilité HTTP seule ne signifie pas que les tarifs et profils nécessaires à l'émission réelle sont actifs. Branche locale `monarque/sel-v2-backend`, HEAD de départ `8c1ae14`; les changements restent non commités et les modifications préexistantes du worktree ont été conservées.

### Retrait du mode métier hors connexion du client Windows — 5 novembre 2026

La distribution Windows V2 est désormais orientée exclusivement vers l'API centrale : écran de connexion obligatoire, opérations via `/v1`, verrouillage si le serveur est inaccessible, retrait de l'administration des appareils/autorisations hors ligne et suppression de l'agent `sel-local.exe` de la configuration et du workflow Tauri. Les fixtures de contrôle visuel représentent maintenant des réponses centrales. Le workflow qui produisait l'ancien installateur V2 Python est supprimé; le workflow Windows V1 est remplacé par une suite de régression sans étape de compilation/publication. Les anciens lanceurs `desktop.py`/`desktop_v2.py` et scripts de build Python/PyWebView sont maintenant bloqués à l'exécution; leurs sources et paquets WiX sont gardés uniquement pour référence historique. La définition du workflow V1 a été conservée sous `.github/legacy/` et n'est pas chargée par GitHub Actions.

| Élément | État | Preuve / limite |
|---|---|---|
| Écrans et pont natif du client | IN_PROGRESS — code modifié | `v2/windows-app/src/App.tsx`, `api.ts`, `src-tauri/src/lib.rs`; l'agent, les routes locales et les états `LOCAL_PENDING` ne sont plus utilisés dans les parcours Windows. La génération de PDF reste côté poste depuis les données centrales, comme prévu. |
| Installateur Tauri | IN_PROGRESS — configuration modifiée | `tauri.conf.json` n'embarque pas l'agent; le workflow ne compile plus `sel-local.exe`. Un binaire généré précédemment dans `src-tauri/resources/sel-local.exe` a été supprimé. Aucun nouvel installateur n'a été construit dans cette tranche. |
| Chaînes de distribution historiques | IN_PROGRESS — neutralisées dans les sources | `.github/workflows/sel-v2-windows.yml` est retiré; `.github/workflows/windows.yml` ne construit plus l'application V1 locale. `build-windows.ps1` et `v2/packaging/build-v2.ps1` s'arrêtent avec un message de retrait; les anciens lanceurs ne démarrent plus leur runtime SQLite. La recette legacy vérifie maintenant la conservation de marqueurs de session/préférences sans créer SQLite, identité de poste ou outbox. |
| Vérification CI et poste Windows | NOT_TESTED | Les tests, le build du client, le nouvel installateur et son démarrage sur le poste SEL n'ont pas été exécutés dans cette tranche. La version déjà installée ne change pas automatiquement : elle doit être remplacée par un nouveau build. |
| Sources historiques SQLite/synchronisation | CONSERVÉES, non distribuées par le client Windows | Les crates Rust anciennes, schémas et tests restent présents pour préserver l'historique et la suite backend. Les routes de synchronisation centrales existent encore dans le code API; aucune suppression de données ou migration n'a été effectuée. Cette conservation technique ne donne pas au nouveau client un mode métier hors connexion. |

**Statut :** le mode local est retiré des parcours distribuables et les lanceurs/builds historiques sont bloqués dans les sources. Les API/crates de synchronisation et leurs migrations restent présentes pour préserver l'historique backend; elles ne sont plus utilisées par le client Windows. Ces modifications n'ont pas encore été validées par CI ni par un nouvel installateur. La copie déjà installée chez SEL n'est pas modifiée par un changement de source : elle continuera à afficher son ancien comportement jusqu'à installation d'un paquet Tauri reconstruit et validé.

### 403 au chargement du tableau de bord — 5 novembre 2026

La revue de la capture et du contrôle d'accès identifie une route manquante dans la matrice RBAC : le client demande `GET /v1/solar/sales` au chargement du tableau de bord, mais cette lecture n'était pas déclarée dans `role_allows_request`. La politique refusait donc la route par défaut, y compris pour le rôle `ADMIN`; le problème concernait également les autres rôles de lecture. Le backend renvoie un `ProblemDetails` RFC avec `code/title/detail`, alors que le client ne lisait que `message`, ce qui réduisait l'erreur à un HTTP 403 générique.

La matrice autorise maintenant la liste et le document des ventes solaires à tous les rôles de lecture déjà définis. Un test parcourt l'ensemble des routes métier GET du client Windows pour ADMIN, BILLING_AGENT, CASHIER, SUPERVISOR et AUDITOR_READONLY. Le client interprète maintenant les champs du `ProblemDetails` et indique le code ainsi que la route en cas de 403.

| Vérification | Résultat |
|---|---|
| Tests unitaires `sel-api --lib` | PASS — 12/12, dont matrice complète des routes de lecture |
| Build client `npm run build` | PASS — bundles générés; avertissement existant de taille des bundles PDF |
| Déploiement de l'API de production | NOT_DONE — le changement doit être déployé sur le serveur pour supprimer le 403 réel |
| Nouvel installateur Windows | NOT_BUILT après cette correction — nécessaire uniquement pour embarquer le diagnostic détaillé côté interface; la correction d'autorisation est serveur |

**Conclusion :** le défaut de code source qui bloque le tableau de bord est corrigé et couvert pour les cinq rôles. La capture ne permet pas à elle seule d'identifier le contenu exact du corps d'erreur reçu en production; toutefois la route non autorisée correspond exactement à l'appel effectué par le client. Le 403 ne disparaîtra chez les utilisateurs qu'après déploiement du backend corrigé. Aucun compte ni donnée de production n'a été modifié.

### Raccordements sans compteur physique — 5 novembre 2026

Le dossier abonné expose désormais la gestion des services Eau et Électricité. Un raccordement se crée sans numéro de compteur; son état peut rester « Inconnu » jusqu'à vérification. L'enregistrement d'un compteur est une action distincte, disponible seulement lorsqu'un appareil physique existe. La page Relevés explique qu'un relevé d'index nécessite un compteur et propose d'ouvrir le dossier abonné pour rattacher le service ou la Facturation pour saisir les quantités relevées manuellement. La facture combinée reste calculée par le moteur Rust et affiche toujours les deux services.

| Vérification | Résultat |
|---|---|
| Gestion UI du service dans le dossier abonné | PASS — sélection Eau/Électricité, état de raccordement, contrat et date facultatifs; aucun numéro fictif |
| Compteur | PASS — création séparée; numéro réel obligatoire uniquement pour cette action |
| Autorisations API | PASS — création/modification des raccordements accessible aux rôles ADMIN, SUPERVISOR et BILLING_AGENT |
| Facturation sans compteur | PASS — la saisie manuelle de kWh et m³ utilise l'API de prévisualisation/émission; aucun identifiant compteur n'est requis |
| Régression HTTP PostgreSQL | CODE — test de liste de compteurs vide ajouté après création d'un raccordement; compilé avec `cargo test -p sel-api --test postgres_meter_readings --no-run -j 1`. Exécution réelle non faite faute de base de test dédiée dans cette session |
| Vérifications locales | PASS — `npm run build`; `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace -j 1`; `python -m pytest -q` (324 réussis); recette Playwright Chromium (29 captures, dont le parcours de rattachement sans compteur, relevés et PDF A4) |
| État des données SEL | PASS — aucune écriture en production; le rattachement des services se fait depuis le dossier de chaque abonné après distribution du client validé |

**Limite de livraison :** les scénarios PostgreSQL annotés `ignore` exigent un `TEST_DATABASE_URL`; ils ne sont pas comptés comme exécutés. La mise à jour du dépôt ou une CI réussie ne remplace pas la publication d'un nouvel installateur ni sa vérification sur le poste de Clarisse.

### UX3 — intégration de la navigation et des parcours CRUD — 6 novembre 2026

Branche technique : `monarque/sel-v2-ux3-implementation-20261106`, dérivée de la branche documentaire `monarque/sel-v2-ux3-codex-handoff-20261106`. Cet incrément n'est ni fusionné ni déployé. Le travail UX3 est empilé sur la branche de handoff afin de préserver la séparation des changements documentaires et de code.

Référence Figma : [SEL V2 — Product Design · Windows & Facturation](https://www.figma.com/design/EuIpbci6IR7m6ggApo5WWm/SEL-V2-%E2%80%94-Product-Design-%C2%B7-Windows---Facturation). L'inventaire des pages 16–17 comprend 76 frames. La vérification visuelle directe a porté sur H00, C01, F05, F01, O06 et S00 ; elle ne constitue pas une validation visuelle exhaustive des 76 frames ni une recette sur le poste SEL.

| Module / exigence | État | Fichiers / preuve / limite |
|---|---|---|
| Navigation par domaines et sous-sections | COMPLETED — code | `v2/windows-app/src/App.tsx`, `App.css` : Accueil, Abonnés, Facturation (Factures/Relevés), Paiements (Journal de caisse/Créances), Solaire et Administration (Administration/État du serveur). L'état de diagnostic reste accessible séparément. Barre latérale et en-tête sont rapprochés du shell H00 de Figma. |
| Tableau de bord | COMPLETED — code compilé | Actions rapides et repères d'utilisation alignés au parcours H00 ; l'action d'administration n'est exposée qu'au rôle ADMIN. Le build ne remplace pas la vérification visuelle dans l'application Windows. |
| Abonnés et navigation contextuelle | COMPLETED — code compilé | Parcours fiche vers relevés, paiement, facture et modification ; l'abonnement est transmis et présélectionné aux vues aval. Les pages de liste suivent `X-Next-Cursor`, avec détection de curseur répétitif. |
| Conservation des saisies | COMPLETED — code compilé | Les formulaires métier marqués suivent les changements non enregistrés et demandent confirmation avant navigation. Après validation serveur, l'état de saisie est réinitialisé. |
| Mutations comptables et garde-fous | COMPLETED — code compilé | Annulation facture avec motif, contre-écriture de paiement avec motif/montant, avoir sur créance ; confirmation et restrictions de rôle dans l'interface, l'autorisation effective restant celle du serveur Rust. Les changements sont append-only via les routes existantes. |
| Cycle de vie abonné | COMPLETED — code compilé | Archivage/réactivation par PATCH avec `If-Match`; confirmation explicite, historique conservé et aucun effacement physique. |
| Raccordement / compte de service | COMPLETED — code compilé | Modification de l'état du service, du contrat et de la date d'activation via PATCH avec révision `If-Match`; un conflit HTTP 409 garde la saisie et invite à recharger/comparer. |
| Contrat OpenAPI d'annulation | COMPLETED — contrat testé | `POST /v1/invoices/{id}/void` précise maintenant le corps JSON `{reason}` dans `v2/openapi/sel-v2.yaml`; assertion de non-régression dans `v2/crates/sel-contracts/tests/openapi_contract.rs`. Aucun endpoint ni pouvoir métier nouveau n'est inventé. |
| Encodage des recherches et pagination du pont Tauri | COMPLETED — code compilé | `src-tauri/src/lib.rs`, `src/api.ts` : le pont transmet `X-Next-Cursor`; accepte les recherches correctement encodées et rejette les chemins encodés, contrôles et échappements mal formés. Routes/méthodes restent sous liste blanche. |
| Ressource de marque Figma | COMPLETED — asset local | `v2/windows-app/public/assets/sel-logo-ux3.svg`, export vectoriel provenant de la ressource Figma. Pas de ressource distante requise à l'exécution. |
| B1 — correction/remplacement de compteur | BLOCKED_API | Figma C10 prévoit cette action, mais l'API ne fournit pas de route de remplacement/désactivation avec préservation de l'historique. Aucun bouton d'action fictif n'est ajouté. Décision métier et API Rust append-only restent nécessaires. |
| B2 — rectification/annulation vente solaire | BLOCKED_API | Aucune route explicite de rectification/annulation coordonnant vente, créance, contrat et paiements. Aucun contournement par suppression ou modification cachée. |
| B3 — avenant/résiliation contrat solaire | BLOCKED_API | L'API ne garantit pas la mutation d'un contrat actif ni le traitement des montants déjà payés. À définir côté contrat métier/Rust avant de proposer l'action. |
| B4 — édition/abandon de brouillon central | BLOCKED_API | Les mutations de DRAFT central ne sont pas garanties par les contrats existants ; aucune action présentée comme fonctionnelle. |
| Build React/TypeScript | PASS | `npm ci` puis `npm run build` dans `v2/windows-app` ; build Vite 8.3.5 réussi. Avertissement non bloquant de chunks PDF/polices dépassant 500 kB. |
| Formatage Rust | PASS | `cargo fmt --all -- --check` dans `v2/windows-app/src-tauri`. |
| Analyse statique Tauri | PASS | `cargo clippy --all-targets -- -D warnings` dans `v2/windows-app/src-tauri`, après regroupement des paramètres de `send_central_admin` dans une structure de requête. |
| Contrat OpenAPI | PASS | `cargo test -p sel-contracts --test openapi_contract` dans `v2/` : 1 test réussi. |
| Tests unitaires Tauri | PARTIAL | `cargo test --lib` a réussi 5 tests avant la dernière refactorisation interne de `send_central_admin`. Après celle-ci, le formatage et Clippy compilent le code final, mais une nouvelle exécution des tests n'a pu aboutir : compilation interrompue par `os error 112` (espace disque insuffisant). Ne pas compter ce résultat comme un PASS sur le dernier état. |
| Tests de contrat UI / capture synthétique | PARTIAL | Après adaptation des anciennes assertions aux domaines UX3, `py -3.12 -m pytest -q tests/test_v2_tauri_client.py` : **52 passed** localement. Ce sont des tests de contrat source, pas un test E2E de clics/focus. Le premier workflow Windows a révélé les assertions obsolètes; une relance après correction est en attente. Le workflow contient aussi une capture synthétique Windows à la taille des frames Figma. |
| Recette Windows/SEL, impression et données réelles | BLOCKED_EXTERNAL | Requiert installation et parcours sur un poste Windows représentatif ; aucune donnée réelle, API de production ou imprimante n'a été touchée dans cet incrément. |
| CI distante et PR UX3 | PARTIAL — PR #60 ouverte | PR brouillon [#60](https://github.com/jupi-z/SEL-Facturation-Desktop/pull/60), empilée sur la branche de handoff (PR #59), commit `91e55b89d1d775ae2b8f54bccf981bff2cb67582`. Workflow backend [42138234484](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42138234484) : PASS, fmt, Clippy, workspace et tests PostgreSQL. Premier workflow Windows [42138333306](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42138333306) : FAIL sur six anciennes assertions statiques après les changements UX3; aucun échec de compilation n'a été atteint. Assertions mises à jour sans supprimer les tests; relancer Windows sur le nouveau commit. |

**État de livraison de cet incrément :** le build web, le contrat OpenAPI, le formatage et Clippy sont validés. Les tests unitaires Tauri avaient réussi avant le dernier petit refactoring, mais n'ont pas pu être relancés après celui-ci en raison de l'espace disque. Les vérifications UI E2E, les captures réelles de l'application, le test poste SEL et la CI de la nouvelle branche restent ouverts. Les lacunes B1–B4 sont des limites des contrats API, et ne sont pas dissimulées par l'interface. Verdict UX3 actuel : **PARTIAL / NO_GO pour considérer tous les parcours UX/CRUD terminés** jusqu'à validation Windows et traitement des quatre gaps API.

#### Mise à jour de validation locale — 6 novembre 2026

L'échec Playwright de la CI Windows au commit `5a8c35f` a été reproduit et isolé. Le script de capture utilisait le port Vite `1420` sans `--strictPort` : lorsqu'un ancien serveur occupait ce port, les captures naviguaient vers cette application périmée alors qu'un nouveau serveur avait démarré ailleurs. Après isolement du port, la recette a révélé trois hypothèses de test obsolètes : la route client paginée inclut une query string, les formulaires non enregistrés demandent une confirmation avant navigation, et le libellé « Administration » existe dans le menu comme dans l'onglet. Le script vise maintenant un port libre dédié, fournit le jeu de données pour la route paginée, accepte les confirmations dans son navigateur exclusivement synthétique et cible explicitement les menus/onglets.

| Vérification actualisée | Résultat | Preuve / limite |
|---|---|---|
| Recette Playwright desktop synthétique | PASS local | `py -3.12 v2/windows-app/scripts/visual_review.py` : parcours complet réussi, 1440×900, 31 captures PNG incluant login, vues métier, CRUD client, rattachement sans compteur, PDF A4, reçus, solaire et administration. Données toutes fictives; ce n'est pas un lancement de l'exécutable Tauri. Captures locales dans `%TEMP%\sel-ux3-final-20261106-c`. |
| Contrats source client | PASS local | `py -3.12 -m pytest -q tests/test_v2_tauri_client.py` : 53 passed, dont la régression de taille/alignement de la case d'administration. |
| Build React/TypeScript | PASS local | `npm run build` dans `v2/windows-app`; avertissement non bloquant de taille des bundles PDF/polices (>500 kB). |
| Syntaxe du script visuel | PASS local | `py -3.12 -m py_compile v2/windows-app/scripts/visual_review.py`. |
| CI Backend sur commit `5a8c35f` | PASS distant | Workflow [42138810759](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42138810759) : success. |
| CI Windows sur commit `567d72f` | PASS distant | Workflow [42140838864](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42140838864) : tests UI, 31 captures desktop, build React, tests natifs Tauri, build NSIS précédent, builds MSI/NSIS et vérification de mise à niveau/préservation du profil : tout est réussi. L'artefact est un candidat CI non signé, pas une release. |
| Tests Rust unitaires Tauri après le refactoring | PASS Windows CI | Le test natif `cargo test --lib --target x86_64-pc-windows-msvc` a réussi dans le workflow Windows [42140838864](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42140838864). Le précédent manque d'espace local n'invalide pas cette exécution distante. |
| Installation native, impression et poste SEL | BLOCKED_EXTERNAL | La capture Playwright s'exécute dans Chromium avec le pont Tauri simulé et des fixtures synthétiques; elle ne remplace pas une installation Windows réelle, une imprimante ou la recette SEL. |

**Résultat CI final du commit code `567d72f6c3aa131f38817f9362ff382fc10f63a6` :** workflow Backend [42140827913](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42140827913) PASS; workflow Windows [42140838864](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42140838864) PASS en 16 min 27 s, incluant l'installation/mise à niveau dans un profil isolé. Le workflow Windows a également téléversé le candidat binaire et les captures synthétiques comme artefacts d'action. Aucune release n'a été créée et aucun artefact n'a été distribué à SEL. La recette sur poste SEL/imprimante demeure `BLOCKED_EXTERNAL`; les limites API B1–B4 demeurent ouvertes. Ce rapport est un commit documentaire ultérieur au code testé et n'altère pas les sources du SHA validé.

#### Revalidation UX3 — confirmations intégrées — 6 novembre 2026

Les confirmations navigateur natives ont été remplacées par une boîte de dialogue intégrée au client pour l'archivage/réactivation d'un abonné et la perte de saisies non enregistrées. Le dialogue expose `role=alertdialog`, reçoit le focus sur « Annuler », piège la tabulation, se ferme avec Échap ou un clic extérieur et restitue le focus au contrôle déclencheur. Les actions destructrices logiques ont un traitement visuel distinct ; aucune suppression physique ni requête métier n'est lancée lors d'une annulation.

| Vérification | Statut | Preuve |
|---|---|---|
| Build TypeScript/Vite | PASS | `npm run build` dans `v2/windows-app`, `tsc -b` et Vite réussis. Vite signale des bundles PDF supérieurs à 500 kB ; c'est une optimisation de chargement différable, sans échec de compilation. |
| Contrats UI source | PASS | `python -m pytest tests/test_v2_tauri_client.py -q` : **54 passed**. Le test ajouté interdit le retour de `window.confirm` et vérifie les attributs/accessibilités attendus. |
| Revue visuelle interactive | PASS — données synthétiques | `python v2/windows-app/scripts/visual_review.py` : code de sortie 0. Le parcours vérifie Escape, annulation explicite et restitution du focus. Captures dans `artifacts/ux3-confirmation-validation-rerun/`, dont `customer-archive-confirmation-1440x900.png`. |
| Backend de production | NON TOUCHÉ | Aucune requête ni écriture de production effectuée ; ce lot ne modifie ni données ni configuration distante. |

Ce lot améliore l'incrément UX3, mais ne ferme pas à lui seul la mission : la comparaison intégrale des parcours Figma, les lacunes API B1–B4, la CI sur le nouveau commit et la recette sur matériel SEL restent à valider.

#### UX3 — remplacement atomique d’un compteur — 6 novembre 2026

Le parcours dossier abonné permet maintenant de remplacer un compteur physique sans effacer son historique. L’agent saisit le nouveau numéro et un motif obligatoire, puis confirme l’opération. L’ancien compteur est désactivé; le nouvel appareil est créé sur le même compte de service et conserve la même unité. Les index et factures antérieurs restent associés à l’ancien identifiant. Détails de décision dans `Docs/SEL_V2_UX3_CODEX/ADR-001-remplacement-compteur.md`.

| Vérification | Résultat | Preuve / limite |
|---|---|---|
| Transaction métier et audit | CODE | Même transaction PostgreSQL : verrou de l’ancien compteur, désactivation, création du nouveau, deux événements `central_changes` et audit avec motif. |
| API et contrôle de rôle | CODE + TEST | `POST /v1/meters/{id}/replace`; ADMIN, SUPERVISOR et BILLING_AGENT autorisés, CASHIER refusé. Tests unitaires de matrice dans `sel-api`. |
| Historique des relevés | TEST D’INTÉGRATION AJOUTÉ | Le scénario HTTP/PostgreSQL vérifie que les deux relevés (original et correction) restent accessibles après remplacement, et qu’une répétition retourne HTTP 409. Il compile avec `cargo test -p sel-api --test postgres_meter_readings --no-run -j 1`; il n’a pas été exécuté localement, car `TEST_DATABASE_URL` n’est pas configuré. |
| Format Rust | PASS LOCAL | `cargo fmt --all -- --check`. |
| Tests API unitaires | PASS LOCAL | `cargo test -p sel-api --lib -j 1` : 12 réussis. |
| Clippy API + stockage | PASS LOCAL | `cargo clippy -p sel-api -p sel-storage --all-targets -j 1 -- -D warnings`. |
| Contrat OpenAPI | PASS LOCAL | `cargo test -p sel-contracts --test openapi_contract -j 1` : 1 réussi; document YAML accepté et aucune route de stockage PDF. |
| Build React/TypeScript | PASS LOCAL | `npm run build`; TypeScript et Vite réussis. Vite conserve l’avertissement non bloquant sur les bundles PDF/polices volumineux. |
| Tests statiques du client | PASS LOCAL | `python -m pytest tests/test_v2_tauri_client.py -q` : 54 réussis. |
| Recette visuelle desktop synthétique | PASS LOCAL | `python v2/windows-app/scripts/visual_review.py` : code de sortie 0, 33 captures dans `artifacts/ux3-meter-replacement-validation-pass/`. Le parcours remplace l’appareil puis saisit un premier relevé sur le nouveau compteur. Fixtures fictives et pont Tauri simulé; ce n’est ni une installation native ni une recette SEL. |
| Test PostgreSQL réel | PASS CI | Workflow backend [42225723764](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42225723764) sur `ef726a447855c0254c89613b79db7a336f847e9b` : workspace Rust et étape « PostgreSQL persistence and transaction tests » réussis, incluant le test ignoré `postgres_meter_readings` exécuté contre le service PostgreSQL éphémère CI. Aucun serveur de production utilisé. |
| CI Windows | PASS CI | Workflow [42225800664](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42225800664) sur le même SHA : tests client, captures desktop, build React, tests natifs Tauri, création MSI/NSIS et essai de mise à niveau avec préservation du profil tous réussis. L’artefact reste un candidat de test non publié. |
| Backend de production | NON TOUCHÉ | Aucun appel ou changement de données/configuration en production. |

**État UX3 après ce lot :** B1 est implémenté et validé par les tests HTTP/PostgreSQL de CI, ainsi que par le build et la recette Windows synthétique. B2 (rectification/annulation d’une vente solaire), B3 (avenant/résiliation d’un contrat) et B4 (modification/abandon d’un brouillon de référentiel) restent ouverts : leurs règles financières ou de versionnement doivent être établies avant d’exposer une action active. La PR UX3 n’est pas fusionnée; aucune publication ni mise en production n’a eu lieu.
