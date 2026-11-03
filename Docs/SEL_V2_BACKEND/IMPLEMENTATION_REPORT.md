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

Contrôle préliminaire effectué le 3 novembre 2026 sur le SHA `77d1e0983db26c0392ec12364663a299b4a81249` : worktree propre, branche alignée avec son upstream, PR #58 ouverte vers `main`; les workflows Rust et paquet serveur réussis, les workflows Windows encore en cours. Les étapes d’audit Figma, de mapping et d’identification de paquet ont été poursuivies ensuite et leur état courant est consigné dans le rapport de phase 0 ci-dessous. Aucun résultat de spike ni aucune recette Windows n’est présumé.

L’audit de phase 0 a ensuite établi que Figma comprend 40 écrans métier et 5 modèles A4, a relevé les principaux écarts API et a identifié le dernier paquet public V1 ; le paquet installé chez SEL reste inconnu. Le rapport détaillé figure dans [PHASE_0_ETAT_DES_LIEUX.md](PHASE_0_ETAT_DES_LIEUX.md). Progression phase 0 calculée selon les poids consignés dans ce rapport : **65 %**. Le spike Tauri reste non exécuté, car cette session Windows n’a pas Rust/Cargo ni Visual Studio C++ Build Tools.

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
