# Rapport d’exécution UX3 — état au 6 novembre 2026

> Rapport d’étape, non rapport de clôture. Les statuts reflètent les preuves exécutées; la recette complète et la validation Figma de toutes les frames ne sont pas terminées.

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
| F05–F07, C13–C14 facture | aperçu sans écriture, confirmation d’émission, historique, document-data local, annulation avec motif | tests domaine/API et captures facture/PDF; build React local | PARTIAL — comparaison aux frames F06/F07 et recette imprimante à faire |
| F01–F04, C15–C17, C34 paiements | sélection client/créances, récapitulatif avant POST, verrou anti-double clic, allocation, reçu, contrepassation | 57 tests Python; capture payment review; test API/RBAC | PARTIAL — parcours HTTP réel contre PostgreSQL requis en recette |
| F08, C18–C19, C35 créances | consultation, références source, avoir via API | réponse JSON additive, test PostgreSQL en CI, écran visuel | PARTIAL |
| O06–O10, C20–C22, C28 solaire | catalogue/ventes et contrats/échéanciers ont des onglets séparés; transitions avec brouillon protégées; suivi expose les métriques renvoyées par le serveur et son CTA mène à l’encaissement prérempli avec client/créance/prochaine échéance; contrat revu avant POST | Playwright synthétique à 1440×900 et 1280×720 : refus de navigation conserve le brouillon, validation l’abandonne; un seul POST contrat après acceptation; la transition O10 sélectionne l’abonné, la créance et l’échéance sans créer de paiement; captures `evidence/solar-contract-workspace-1440x900.png`, `evidence/solar-payment-prefill-1280x720.png` | PARTIAL — annulation vente et avenant/résiliation contrat non exposés; autres détails O06–O10 à comparer |
| A01–A09, C23–C25, C29 administration | utilisateurs/rôles, catalogues versionnés, approbation/révocation, audit; création d’utilisateur avec confirmation (secret masqué), confirmation des changements de rôle/statut et des approbations/révocations de référentiels; création de brouillon tarifaire et réinitialisation du mot de passe revues avant POST; erreurs 403/409/422 compréhensibles en français sans URL API | recette Playwright synthétique : aucune requête avant confirmation, annulation qui conserve le brouillon, secret masqué dans le récapitulatif, une requête après confirmation, refus 409 d’auto-modification expliqué en français; captures `evidence/admin-catalog-draft-confirmation-1440x900.png`, `evidence/admin-catalog-draft-confirmation-1280x720.png`, `evidence/admin-self-role-conflict-1440x900.png`, `evidence/admin-self-role-conflict-1280x720.png` | PARTIAL — revue de toutes les frames d’administration et recette des erreurs réelles contre PostgreSQL restent à faire |
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
| Workflows GitHub sur `8b20ed6` | Backend, paquet serveur et régression V1 : PASS; Tauri Windows : EN COURS au dernier contrôle. Ce run précède le HEAD `4cb8793`; vérifier le nouveau run après push. |
| `python v2/windows-app/scripts/visual_review.py` à 1440×900 | PASS au code UI `4cb8793` — parcours synthétiques, refus de modification du rôle propre expliqué en français, brouillon de tarif et écrans solaires; aucun test sur serveur SEL réel |
| `python v2/windows-app/scripts/visual_review.py` à 1280×720 | PASS au code UI `4cb8793` — mêmes parcours aux dimensions réduites; captures du refus 409 et de la confirmation de brouillon |
| PostgreSQL local / HTTP contre base réelle | NOT_RUN localement; CI PostgreSQL dédiée PASS au commit cité |
| Impression sur imprimante SEL et installation poste cible | NOT_RUN — aucun poste/imprimante cible utilisé dans cette étape |
| Données réelles / backend production | NON TOUCHÉ — aucune connexion, migration, écriture ou configuration de production |

## 6. Diff et risques

- Changements UX3 : `v2/windows-app/src/App.tsx`, `App.css`, `api.ts`, `src-tauri/src/lib.rs`, tests Python, script Playwright, OpenAPI/API Rust pour les lacunes nécessaires et preuves visuelles. Les créations et modifications d’abonné, rattachements et mutations compteur, contrats solaires, créations d’utilisateur, changements de rôle/statut, approbations et révocations demandent une revue explicite; les secrets restent masqués et les saisies sont conservées si une revue est annulée. L’API crée tout nouvel abonné à l’état `ACTIVE`; l’UI le signale et ne propose pas un choix d’état absent du contrat.
- Migrations : la branche intègre les migrations de `main` déjà publiées dans le code; aucune migration exécutée sur un environnement réel.
- Le dernier changement visuel supprime le tableau d’activité supplémentaire du dashboard, pour respecter la frame H00 et éviter un panneau coupé en bas de fenêtre. Les historiques restent dans leurs écrans.
- Risques résiduels : comparaison exhaustive des 76 écrans non effectuée; recettes 1280×720 et 125/150 % non toutes couvertes; CI Tauri/MSI/NSIS à terminer sur le HEAD courant; opérations B2/B3/B4 sans contrat métier suffisant.
- Retour arrière : revert des commits UX3 ciblés sur la branche; aucune réécriture de données n’est introduite.
- Actions non prises en charge : ne pas afficher/activer annulation d’une vente solaire ni avenant/résiliation du contrat tant que les APIs ne sont pas définies et prouvées.

## 7. Verdict d’étape

| Domaine | État | À faire |
|---|---|---|
| UX / navigation | PARTIAL | comparer les écrans restants aux frames; vérifier retours, clavier et erreurs |
| CRUD / corrections | PARTIAL | parcours API réel de bout en bout; contrats B2/B3; décisions B4 |
| Backend / API | PASS pour les routes touchées au commit CI | garder CI backend verte sur le nouveau HEAD |
| Finance / documents | PARTIAL | recette structurée des paiements/créances, génération et impression physique |
| Build Windows | PARTIAL | terminer CI Tauri MSI/NSIS et confirmer installation/upgrade isolé |
| Poste matériel SEL | NOT_TESTED | installer et exécuter sur poste représentatif; ne pas assimiler Playwright à un test matériel |
| Production | NO-GO / NON TOUCHÉE | hors périmètre de cette PR; aucune bascule production demandée ou faite |

**Décision :** PR #60 reste ouverte et non fusionnée. Aucun installateur, release ou déploiement n’est publié par ce lot. La mission UX3 n’est pas terminée tant que les comparaisons de frames et les validations restantes ne sont pas clôturées.
