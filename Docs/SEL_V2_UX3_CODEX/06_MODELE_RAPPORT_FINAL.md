# Rapport d’exécution UX3 — état au 7 novembre 2026

> Rapport d’étape, non rapport de clôture. Les statuts reflètent les preuves exécutées; la recette complète et la validation Figma de toutes les frames ne sont pas terminées.

## Progression vérifiée — 7 novembre 2026

### F01 — Journal de caisse et séparation de la saisie

- Comparaison directe avec le frame Figma `178:1194` de la page `16 · SEL V2 — Prototype intégral & CRUD`.
- Le journal des règlements et la création d’un paiement sont maintenant deux espaces distincts. Le journal présente les règlements, confirmations, conflits/refus et reçus disponibles à partir des seules réponses centrales, avec le tableau des opérations et les actions suivantes.
- Les raccourcis vers la création ouvrent le formulaire existant sans déplacer les règles financières hors de Rust. Le reçu est ouvert depuis un règlement retourné par le serveur; aucun reçu n’est proposé pour un formulaire non enregistré.
- Un scénario navigateur crée un paiement synthétique après revue et confirmation, retourne au journal puis vérifie l’aperçu du reçu. Aucune écriture n’a été envoyée à la production.
- Captures : `evidence/payment-journal-f01-2026-11-07-1440x900.png` et `evidence/payment-journal-f01-2026-11-07-1280x720.png`.

| Contrôle F01 | Résultat |
|---|---|
| `npm run build` (`v2/windows-app`) | PASS — TypeScript/React compilés; avertissement non bloquant sur les chunks PDF/fonts |
| `python -m pytest tests/test_v2_tauri_client.py -q` | PASS — 65 tests |
| Recette navigateur 1440×900 et 1280×720 | PASS — séparation journal/saisie, paiement simulé confirmé, affichage du reçu PDF, aucun débordement horizontal |
| Captures inspectées visuellement | PASS — les deux résolutions examinées |
| Nature de la recette | API et données synthétiques; aucun paiement réel enregistré |
| CI GitHub de l’incrément | À vérifier après publication du commit |

F01 est implémenté et validé dans l’application de test. Le flux contre PostgreSQL réel, la permission réelle, l’imprimante SEL et le poste cible restent à valider.

### F07 — Historique des factures

- Comparaison directe avec le frame Figma `178:1951` de la page `16 · SEL V2 — Prototype intégral & CRUD`.
- L’historique est maintenant une vue distincte de la saisie : quatre indicateurs calculés depuis la liste du serveur, recherche, tableau des factures et panneau « Actions suivantes ». La création s’ouvre avec « Nouvelle facture » et les raccourcis métier ; le moteur Rust et les routes centrales restent les seules autorités métier.
- Le parcours de prévisualisation conserve le formulaire monté et ses valeurs lorsqu’on revient à la saisie. Après émission confirmée par le serveur, le client revient à l’historique.
- Captures du parcours synthétique : `evidence/invoice-history-f07-2026-11-07-1440x900.png` et `evidence/invoice-history-f07-2026-11-07-1280x720.png`.

| Contrôle F07 | Résultat |
|---|---|
| `npm run build` (`v2/windows-app`) | PASS — React/TypeScript compilés; avertissement non bloquant sur les chunks PDF/fonts |
| `python -m pytest tests/test_v2_tauri_client.py -q` | PASS — 63 tests |
| `python v2/windows-app/scripts/visual_review.py`, 1440×900 et 1280×720 | PASS — navigation, indicateurs, séparation historique/saisie, prévisualisation, conservation du brouillon et documents PDF synthétiques |
| `git diff --check` | PASS |
| Réponse financière de la recette | Fixtures synthétiques seulement; aucun appel ni changement sur la production |
| CI du commit F07 | À vérifier après publication du commit |

F07 est implémenté et validé dans l’application de test; cette preuve ne valide ni l’impression sur l’équipement SEL ni les données du serveur de production.

### F06 — Vérification avant émission

- Comparaison directe au frame Figma `178:1832` : l’étape s’intitule maintenant « Vérifier une facture » et présente le dossier, la période, le nombre de lignes et le TTC calculé par Rust, puis le tableau détaillé et un panneau d’actions.
- Les commandes sont séparées : corriger la saisie, revenir à l’historique (avec confirmation d’abandon si le brouillon a changé) ou confirmer l’émission. Après confirmation, un verrou UI empêche les clics/requêtes répétés pendant l’enregistrement central.
- Le brouillon de saisie reste monté pendant l’aperçu afin que « Corriger la saisie » restaure les valeurs sans perte.
- Captures synthétiques : `evidence/invoice-verification-f06-2026-11-07-1440x900.png` et `evidence/invoice-verification-f06-2026-11-07-1280x720.png`.

| Contrôle F06 | Résultat |
|---|---|
| `npm run build` (`v2/windows-app`) | PASS — TypeScript/React compilés; avertissement non bloquant sur les chunks PDF/fonts |
| `python -m pytest tests/test_v2_tauri_client.py -q` | PASS — 63 tests |
| Recette navigateur 1440×900 et 1280×720 | PASS — aperçu, retour à la saisie sans perte, confirmation d’abandon au retour historique, émission simulée et exactement un POST sous double déclenchement |
| `git diff --check` | PASS |
| Nature de la recette | API et montants synthétiques; aucune facture réelle créée |
| CI couvrant F06 | À vérifier après publication du commit |

F06 est implémenté et validé sur l’application synthétique. L’émission réelle, l’autorisation utilisateur contre PostgreSQL de production et la validation sur poste SEL ne sont pas couvertes par ce scénario.

### F05 — Émission de facture combinée

- Comparaison directe au frame Figma `178:1704` : le formulaire est présenté sous « Émettre une facture combinée », avec l’accès à l’historique et un panneau « Actions suivantes ».
- Le formulaire métier déjà relié au moteur Rust est conservé. L’action de prévisualisation reste bloquée tant que le catalogue ne fournit pas le barème applicable; les tarifs, taxes et montants ne sont pas calculés dans l’interface.
- Aucun KPI fictif n’a été ajouté à l’écran de saisie : les métriques placeholders de la maquette ne représentent pas des données utiles pour préparer une nouvelle facture.
- Captures synthétiques : `evidence/invoice-issue-f05-2026-11-07-1440x900.png` et `evidence/invoice-issue-f05-2026-11-07-1280x720.png`.

| Contrôle F05 | Résultat |
|---|---|
| `npm run build` (`v2/windows-app`) | PASS — TypeScript/React compilés; avertissement non bloquant sur les chunks PDF/fonts |
| `python -m pytest tests/test_v2_tauri_client.py -q` | PASS — 64 tests |
| Recette navigateur 1440×900 et 1280×720 | PASS — ouverture F05 depuis F07, saisie, preview F06, conservation du brouillon, sortie avec confirmation, génération PDF locale et une seule requête d’émission dans le test synthétique |
| `git diff --check` | PASS |
| Nature de la recette | API et données simulées; aucune facture réelle émise |
| CI couvrant F05 | À vérifier après publication du commit |

F05 est implémenté et validé avec le scénario synthétique. Les tarifs de production, les permissions réelles, l’émission PostgreSQL, l’imprimante et le poste SEL doivent encore être vérifiés hors de ce test.

### Isolation des données financières entre abonnés

- Reproduction Playwright : après ouverture du dossier A, passage par Relevés, sélection du dossier B et ouverture immédiate pendant que l’API de B est volontairement suspendue, le relevé A restait visible dans le dossier B.
- Correction : chaque état de relevé porte maintenant l’identifiant de son abonné; le rendu n’expose un relevé que si cet identifiant correspond à l’abonné sélectionné. Le chargement et l’erreur sont affichés distinctement et les anciennes données sont écartées.
- La même protection s’applique au panneau des créances. Aucun endpoint, calcul financier ou état de production n’a été modifié.
- Captures synthétiques : `evidence/customer-statement-switch-2026-11-07-1440x900.png` et `evidence/customer-statement-switch-2026-11-07-1280x720.png`.

| Contrôle de cet incrément | Résultat |
|---|---|
| Reproduction avant correction | FAIL attendu — la facture synthétique de A restait visible pendant le chargement de B |
| `python -m pytest tests/test_v2_tauri_client.py -q` | PASS — 62 tests |
| `npm run build` (`v2/windows-app`) | PASS — TypeScript/React compilés; avertissement non bloquant sur les chunks PDF/fonts |
| `python v2/windows-app/scripts/visual_review.py`, 1440×900 et 1280×720 | PASS — bascule A→B avec réponse B retardée, absence du document de A, arrivée du document de B; reste de la recette synthétique exécuté |
| `git diff --check` | PASS |
| CI GitHub du parent `224f97261c6718b8d848af00094b0ebe59dc045c` | PASS — backend [42307206930](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42307206930), serveur [42307206491](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42307206491), régression V1 [42307206533](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42307206533), Tauri Windows/installeurs/upgrade [42307206466](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42307206466) |
| CI couvrant ce correctif précis | À relancer après push du commit de cet incrément |

La recette visuelle utilise des fixtures synthétiques et ne constitue pas une validation sur les données SEL ni sur le poste cible.

### Administration A01 / A02 / A03

- A01 sépare le répertoire, la recherche, les indicateurs et les actions suivantes; les modifications de compte sont déplacées dans une fiche de sécurité dédiée.
- A02 présente la création d’un utilisateur dans un écran distinct. Une identité déjà utilisée déclenche un conflit API `USER_IDENTITY_EXISTS`, traduit en français, sans exposer le détail brut ni la route centrale.
- A03 regroupe rôle, état et réinitialisation du mot de passe. Le compte de la session courante ne peut pas changer son propre rôle ni désactiver son propre compte. Ces contrôles sont désactivés dans le DOM et visuellement distincts des actions disponibles.
- Le refus d’auto-modification continue d’être couvert par le test unitaire de traduction de l’erreur centrale; la recette navigateur vérifie désormais le verrouillage préventif et teste à la place un conflit de doublon réaliste.
- Les confirmations métier sont vérifiées au clavier : focus initial dans la boîte, Tabulation/Shift+Tab bouclés dans la boîte, Échap ferme sans écrire et rend le focus au déclencheur.
- Les erreurs de navigation préservent le brouillon et demandent une confirmation avant abandon. Les captures visuelles montrent la page entière.

### Preuves locales

| Contrôle | Résultat au 7 novembre |
|---|---|
| `python -m pytest tests/test_v2_tauri_client.py -q` | PASS — 62 tests |
| `npm run build` (`v2/windows-app`) | PASS — build React/TypeScript; avertissement non bloquant sur les gros chunks PDF/fonts |
| `python v2/windows-app/scripts/visual_review.py`, fenêtre 1440×900 | PASS — parcours synthétiques complets, conflits doublon, verrouillage des droits propres et apparence calculée des boutons désactivés |
| Même recette, fenêtre 1280×720 | PASS — mêmes parcours; aucune surlargeur horizontale détectée |
| Confirmation clavier, 1440×900 et 1280×720 | PASS — focus initial, Tab/Shift+Tab, Échap, aucune requête d’archivage et focus restauré |
| PostgreSQL réel SEL, production, imprimante et poste utilisateur | NOT_RUN — aucun accès ni test réel dans ce lot |

Captures reproductibles dans `evidence/` : `admin-user-directory-2026-11-07-1440x900.png`, `admin-duplicate-user-conflict-2026-11-07-1440x900.png`, `admin-self-access-locked-2026-11-07-1440x900.png`, `customer-archive-keyboard-focus-2026-11-07-1440x900.png`, et leurs versions 1280×720. Les anciennes captures nommées `admin-self-role-conflict-*` documentent un scénario de test antérieur et ne représentent plus le parcours actuel.

### État CI au SHA poussé `224f97261c6718b8d848af00094b0ebe59dc045c`

- PASS : backend — [42307206930](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42307206930).
- PASS : paquet serveur — [42307206491](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42307206491).
- PASS : régression V1 — [42307206533](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42307206533).
- PASS : client Tauri Windows, compilation MSI/NSIS et test isolé installation/mise à niveau — [42307206466](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42307206466).

### État de livraison

- Ces changements restent sur la branche UX3 et la PR #60, sans fusion dans `main`, publication ni déploiement.
- Les statuts CI ci-dessus couvrent le SHA `224f972`; les modifications d’isolation décrites ci-dessus attendent encore leur propre CI après push.
- Le découpage A01–A03 améliore cette zone seulement. L’ensemble UX3 reste PARTIAL et la comparaison des 85 frames, les recettes des autres parcours et les limites API métier restent à traiter.

## 1. Références d’exécution

- Dépôt : `jupi-z/SEL-Facturation-Desktop`
- Branche : `monarque/sel-v2-ux3-implementation-20261106`
- Base intégrée : `main@0e29e7712e3b0b167ac8729d2a2e78e3ae612cd6`
- Derniers commits UX3 : `d74166df21a0b3f7a07ab917aef8a1a9bda615d7` (H00 / responsive), `d9ca03ad7296066483295e9c7536e1472b2bb2b8` (confirmations administration), `d6d20a3829d6a0f85a7cc43b9c7e2e6d15971e6f` (preuve admin), `342f456913ce645a1fc91c8fa4ce223b8a7d423e` (approbation/révocation référentiels), `3c8f71ccf6a02c503c337a95ab10a76eabdb3935` (confirmation création abonné), `4dec135768a35e3b90bbd000dca0e87e18269a64` (confirmation édition abonné), `1958fe3cc4fd1d8a1dd48397250edbbd27ee515f` (confirmations services et compteurs), `5689f652ca193eab5474c216eb7622c7409338d8` (preuves navigateur complémentaires), `e2135e6330db819fe019cf8ec0d9fc77da4d3dbb` (revue du contrat solaire), `a488937aa100325e590c0785aa742db962e02085` (rapport solaire), `9a791f3caa859a5bd7a12f54d433f666d3b0885f` (navigation solaire dédiée selon O09), `2764eb68c743b0ca0c32fd244d42d65885db0a58` (paiement prérempli depuis O10), `ce464c4382e2dcca4835074596704b40eacc6afd` (confirmation des créations de brouillon tarifaire et réinitialisation du mot de passe utilisateur), `8b20ed6823573abb0e9108f278e2f3d8bab448b2` (rapport administration), `4cb8793` (erreurs API centralisées en français et recette du refus d’auto-modification de rôle).
- PR #60 : https://github.com/jupi-z/SEL-Facturation-Desktop/pull/60 — ouverte, non fusionnée, base `main`.
- Figma page 16 (`177:1481`, « 16 · SEL V2 — Prototype intégral & CRUD ») : noms et structure des 76 écrans principaux et 9 overlays (85 frames au total) inventoriés; contextes haute fidélité consultés pour H00, A02, C01, F02, F05, O06, O09 (`178:3249`) et O10 (`178:3378`). O09 a un espace contrats distinct; O10 prévoit une action d’encaissement et affiche les valeurs du serveur. Le client relie maintenant le suivi du contrat au journal de caisse en transmettant l’abonné, la créance existante et la prochaine échéance ouverte. La comparaison visuelle complète de toutes les frames reste à faire.
- Figma page 17 : les trois frames de documentation recensées; contexte haute fidélité lu pour `191:76` (écarts API et décisions). Les exigences sont croisées au dossier UX3 du dépôt.
- Environnement local : Windows 11 Professionnel, Python 3.12.10, Node 24.21.0, Rust 1.99.1, Cargo 1.99.1. Le test natif Tauri local a compilé sous MSVC.
- PostgreSQL local : aucun `TEST_DATABASE_URL`; les tests transactionnels sont exécutés par le workflow GitHub avec son PostgreSQL isolé.

## 2. Invariants protégés

| Invariant | Preuve | État |
|---|---|---|
| Facture combinée, ligne eau visible à 0 m³ | tests Rust de facturation et parcours visuel synthétique | DONE |
| Calculs financiers et soldes issus du serveur Rust | aperçu et reçus UI utilisent les valeurs API; tests API Rust/CI | DONE pour les parcours exercés |
| PDF généré localement, sans route de stockage serveur | test OpenAPI `openapi_document_is_valid_yaml_and_declares_no_pdf_route`; revue du rendu PDF local | DONE |
| Snapshots, références et écritures historiques non altérés | changements append-only des fonctions financières; aucune migration UX de données | DONE pour ce lot |
| Sessions, RBAC et whitelist Tauri conservés | 12 tests `sel-api`, 5 tests natifs Tauri; CI backend verte | DONE pour les routes testées |
| Pas d’opération métier hors connexion dans ce client | architecture connectée et indication serveur dans l’application | DONE |
| Pas de suppression physique financière | UI présente annulation/contrepassation/avoir audités | DONE pour les actions exposées |
| Migrations/SafeUpdate non exécutés sur données réelles | aucun accès production ni commande de migration | DONE — production intacte |

## 3. Parcours implémentés

| Domaine / frames | Implémentation et API | Preuves | État |
|---|---|---|---|
| H00 et navigation six domaines | Accueil, raccourcis, session centrale, contexte/retour; tableau de bord rapproché de la frame H00 | captures 1440×900; recette browser 34 captures; le correctif enlève le tableau hors-maquette au bas de l’accueil | PARTIAL — H00 traité, toutes les frames non comparées |
| C01–C06, C30–C31 abonnés | création, recherche, dossier, modification, archivage/réactivation; création et édition relues avant écriture; état initial `ACTIVE` annoncé comme fixé par l’API | Playwright synthétique : annulation sans POST/PATCH, saisie conservée, confirmation avec un seul appel; captures `evidence/customer-create-confirmation-1440x900.png`, `evidence/customer-edit-confirmation-1280x720.png` | PARTIAL — état initial non configurable par le contrat API; conflit 409 réel à valider de bout en bout |
| O01–O03, C07–C10, C32 services/compteurs | services avec ou sans compteur; rattachement, ajout et remplacement non destructif relus avant écriture; édition du raccordement relue avant PATCH | revue Playwright à 1440×900 et 1280×720; annulation sans requête et saisie préservée; captures `evidence/service-account-confirmation-1280x720.png`, `evidence/meter-create-confirmation-1280x720.png`, `evidence/service-account-edit-confirmation-1280x720.png`; test HTTP/PostgreSQL de remplacement inclus dans CI backend | PARTIAL — détails Figma à contrôler sur chaque écran |
| O04–O05, C11–C12, C33 relevés | saisie, aperçu, historique et correction append-only | visual review; tests API backend en CI | PARTIAL |
| F05–F07, C13–C14 facture | émission, vérification et historique rapprochés des frames Figma; confirmation explicite, conservation du brouillon, document-data local, annulation avec motif | 65 tests client; captures F05/F06/F07 1440×900 et 1280×720; recette PDF et émission synthétiques | PARTIAL — API réelle, imprimante cible et validation sur poste SEL restent à faire |
| F01–F04, C15–C17, C34 paiements | journal séparé de la saisie; sélection client/créances, récapitulatif avant POST, verrou anti-double clic, reçu, contrepassation | 65 tests Python; captures journal/revue/reçu; test API/RBAC | PARTIAL — parcours HTTP réel contre PostgreSQL requis en recette; F02–F04 restent à comparer |
| F08, C18–C19, C35 créances | consultation, références source, avoir via API | réponse JSON additive, test PostgreSQL en CI, écran visuel | PARTIAL |
| O06–O10, C20–C22, C28 solaire | catalogue/ventes et contrats/échéanciers ont des onglets séparés; transitions avec brouillon protégées; suivi expose les métriques renvoyées par le serveur et son CTA mène à l’encaissement prérempli avec client/créance/prochaine échéance; contrat revu avant POST | Playwright synthétique à 1440×900 et 1280×720 : refus de navigation conserve le brouillon, validation l’abandonne; un seul POST contrat après acceptation; la transition O10 sélectionne l’abonné, la créance et l’échéance sans créer de paiement; captures `evidence/solar-contract-workspace-1440x900.png`, `evidence/solar-payment-prefill-1280x720.png` | PARTIAL — annulation vente et avenant/résiliation contrat non exposés; autres détails O06–O10 à comparer |
| A01–A09, C23–C25, C29 administration | répertoire, création et sécurité séparés; rôles/statuts verrouillés pour le compte courant; création, changement de rôle/statut, réinitialisation et mutations de référentiels relus avant écriture; erreurs 403/409/422 localisées sans URL API | recette Playwright synthétique : conflit identité doublon, droits propres désactivés, apparence disabled, dialogue clavier avec focus trap/Échap/restauration; preuves datées `admin-user-directory-*`, `admin-duplicate-user-conflict-*`, `admin-self-access-locked-*`, `customer-archive-keyboard-focus-*`, approbation et brouillon | PARTIAL — revue de toutes les frames d’administration et recette des erreurs réelles contre PostgreSQL restent à faire |
| E01–E11, C26–C27 états | états auth/erreurs, confirmation intégrée, validation et reprise | tests UI et script browser | PARTIAL — recette exhaustive 401/403/409/422/503 encore requise |

## 4. Lacunes API et décisions

| Code | État actuel | Décision sûre | Statut |
|---|---|---|---|
| B1 compteur | `POST /v1/meters/{id}/replace` est implémentée et protégée; ancien compteur conservé, nouveau créé transactionnellement | aucune suppression physique | DONE techniquement; tests CI à garder verts |
| B2 vente solaire | pas d’API de rectification/annulation reliant vente, créance, paiements et audit | ne pas inventer d’écriture comptable; définir contrat métier Rust puis tester avant d’activer une action UI | BLOCKED_API |
| B3 contrat actif | pas d’avenant/résiliation avec politique de traitement des sommes reçues | exiger une décision métier sur principal, paiements et échéances; aucun recalcul côté client | BLOCKED_API |
| B4 brouillons admin | création, approbation et révocation existent; modification/abandon explicite d’un brouillon serveur non garanti | utiliser une nouvelle version rectificative; ne pas détourner DELETE | PARTIAL / BLOCKED_API si édition ou abandon serveur requis |

## 5. Tests et preuves observés

| Commande / scénario | Résultat réel |
|---|---|
| `python -m pytest tests/test_v2_tauri_client.py -q` | PASS — 61 tests au code `4cb8793`, y compris navigation solaire confirmée/refusée, préremplissage O10, revue des brouillons tarifaires, réinitialisations et traduction des erreurs centrales |
| `npm run build` (`v2/windows-app`) | PASS au code applicatif `4cb8793`; avertissement non bloquant pour les chunks PDF/fonts volumineux |
| `cargo fmt --manifest-path v2/Cargo.toml --all -- --check` | PASS |
| `cargo test --manifest-path v2/Cargo.toml -p sel-api --lib` | PASS — 12 tests |
| `cargo test --manifest-path v2/Cargo.toml -p sel-contracts --test openapi_contract` | PASS — 1 test |
| `cargo test --manifest-path v2/windows-app/src-tauri/Cargo.toml --lib` | PASS — 5 tests whitelist/session Tauri |
| `cargo test --manifest-path v2/Cargo.toml --workspace` local | FAIL d’environnement de build, LNK1207/PDB incompatible à l’édition de liens de `postgres_catalog_api`; aucune assertion rapportée en échec |
| Workflows GitHub sur `d74166d` | PASS — backend, serveur package, régression V1, build MSI/NSIS et installation/upgrade isolé avec profil préservé. Cette preuve ne couvre pas les modifications d’administration ultérieures. https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42283573787 · https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42283573819 · https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42283573823 · https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/42283573796 |
| Workflows GitHub au SHA `37d1305b93829e1f50e9d950b1f0c214e115f2af` | PASS — backend, paquet serveur, régression V1, client Tauri Windows avec test d’installation/mise à niveau isolé. Ce run couvre le commit poussé; les tests clavier ajoutés après ce SHA devront repasser la CI à leur prochain push. |
| `python v2/windows-app/scripts/visual_review.py` à 1440×900 | PASS au code UI `4cb8793` — parcours synthétiques, refus de modification du rôle propre expliqué en français, brouillon de tarif et écrans solaires; aucun test sur serveur SEL réel |
| `python v2/windows-app/scripts/visual_review.py` à 1280×720 | PASS au code UI `4cb8793` — mêmes parcours aux dimensions réduites; captures du refus 409 et de la confirmation de brouillon |
| PostgreSQL local / HTTP contre base réelle | NOT_RUN localement; CI PostgreSQL dédiée PASS au commit cité |
| Impression sur imprimante SEL et installation poste cible | NOT_RUN — aucun poste/imprimante cible utilisé dans cette étape |
| Données réelles / backend production | NON TOUCHÉ — aucune connexion, migration, écriture ou configuration de production |

## 6. Diff et risques

- Changements UX3 : `v2/windows-app/src/App.tsx`, `App.css`, `api.ts`, `src-tauri/src/lib.rs`, tests Python, script Playwright, OpenAPI/API Rust pour les lacunes nécessaires et preuves visuelles. Les créations et modifications d’abonné, rattachements et mutations compteur, contrats solaires, créations d’utilisateur, changements de rôle/statut, approbations et révocations demandent une revue explicite; les secrets restent masqués et les saisies sont conservées si une revue est annulée. L’API crée tout nouvel abonné à l’état `ACTIVE`; l’UI le signale et ne propose pas un choix d’état absent du contrat.
- Migrations : la branche intègre les migrations de `main` déjà publiées dans le code; aucune migration exécutée sur un environnement réel.
- Le dernier changement visuel supprime le tableau d’activité supplémentaire du dashboard, pour respecter la frame H00 et éviter un panneau coupé en bas de fenêtre. Les historiques restent dans leurs écrans.
- Risques résiduels : comparaison exhaustive des 76 écrans non effectuée; mise à l’échelle Windows 125/150 % et recette sur le poste SEL restent à valider; les opérations B2/B3/B4 n'ont pas de contrat métier suffisant.
- Retour arrière : revert des commits UX3 ciblés sur la branche; aucune réécriture de données n’est introduite.
- Actions non prises en charge : ne pas afficher/activer annulation d’une vente solaire ni avenant/résiliation du contrat tant que les APIs ne sont pas définies et prouvées.

## 7. Verdict d’étape

| Domaine | État | À faire |
|---|---|---|
| UX / navigation | PARTIAL | comparer les écrans restants aux frames; vérifier retours, clavier et erreurs |
| CRUD / corrections | PARTIAL | parcours API réel de bout en bout; contrats B2/B3; décisions B4 |
| Backend / API | PASS pour les routes touchées au commit CI | garder CI backend verte sur le nouveau HEAD |
| Finance / documents | PARTIAL | recette structurée des paiements/créances, génération et impression physique |
| Build Windows | PARTIAL | CI MSI/NSIS et test d’installation/mise à niveau isolé PASS au SHA `37d1305`; test sur poste matériel SEL encore requis |
| Poste matériel SEL | NOT_TESTED | installer et exécuter sur poste représentatif; ne pas assimiler Playwright à un test matériel |
| Production | NO-GO / NON TOUCHÉE | hors périmètre de cette PR; aucune bascule production demandée ou faite |

**Décision :** PR #60 reste ouverte et non fusionnée. Aucun installateur, release ou déploiement n’est publié par ce lot. La mission UX3 n’est pas terminée tant que les comparaisons de frames et les validations restantes ne sont pas clôturées.
