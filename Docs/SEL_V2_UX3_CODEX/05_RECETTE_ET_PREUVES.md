# 05 — Matrice de recette, commandes et preuves

## 1. Contrat d'attestation

Chaque ligne exécutée doit indiquer : **ID**, fonction, résultat attendu, environnement et données synthétiques, **commande/scénario**, **SHA**, **PASS / FAIL / NOT_RUN / BLOCKED_EXTERNAL**, preuve et ticket éventuel. Aucune déclaration de « testé » si seule la maquette Figma a été inspectée.

Les scénarios suivants sont obligatoires dans la PR technique, et doivent rester reproductibles sans base SEL réelle. Marquer `NOT_RUN` si non exécuté.

## 2. Cas UX / Navigation

| ID | Test d'acceptation |
|---|---|
| UX-01 | Six domaines accessibles au clavier et à la souris ; domaine actif correctement affiché |
| UX-02 | Accueil → chaque opération principale → retour sans perte de contexte |
| UX-03 | Passage abonné → facture/relevé/paiement avec même `customerId`, sans re-sélection erronée |
| UX-04 | Détail → sous-action → annuler → retour au détail d'origine (filtres conservés) |
| UX-05 | Brouillon non enregistré → changement de section : confirmation de perte et option Continuer |
| UX-06 | 401/403/409/422/503 : message compréhensible, pas de succès fictif, retour/relance adapté |
| UX-07 | Réponse lente : disabled/aria-busy, pas de doublon d'action ni deuxième émission |
| UX-08 | Recherche, tri/pagination, états vides/chargement, 1/100 lignes, longues étiquettes |
| UX-09 | Focus visible, tabulation, Escape modal, annonces d'erreurs, labels, contraste |
| UX-10 | 1440×900, 1280×720, Windows scaling 125/150 %, scroll accessible et aucun CTA hors écran |

## 3. Cas CRUD / Backend

| ID | Séquence / invariant |
|---|---|
| CRUD-01 | Créer abonné → vérifier sauvegarde → ouvrir → modifier via If-Match |
| CRUD-02 | Archive `ACTIVE → ARCHIVED`, reste visible dans filtre archivés ; restauration inverse |
| CRUD-03 | Conflit `409` entre deux versions : champs conservés, comparaison, aucune perte silencieuse |
| CRUD-04 | Service créé sans compteur, puis modifier connection_status avec révision valide |
| CRUD-05 | Compteur : consulter et créer ; toute tentative de correction sans API réelle bloquée et clairement expliquée |
| CRUD-06 | Relevé enregistré : correction append-only avec motif ; ancien et nouveau visibles, droits contrôlés |
| CRUD-07 | Facture preview → issue : eau 0 m³ explicitement présente, électricité kWh, total Rust |
| CRUD-08 | Facture avec eau positive m³ : tarif et taxe approuvés, total Rust et PDF cohérents |
| CRUD-09 | Facture émise : numéro inchangé, PDF local, annulation autorisée seulement avec réponse API valide |
| CRUD-10 | Paiement : choisir client/une créance → contrôle → POST unique, reçu et retour |
| CRUD-11 | Paiement sur plusieurs créances : allocations valides, pas d'inter-abonné ni mélange illégal de devise |
| CRUD-12 | Retry/double clic sur POST : un seul effet durable, clé d'idempotence correctement réutilisée |
| CRUD-13 | Contrepassation totale/partielle selon autorisation Rust, motif et audit, pas de mutation de l'original |
| CRUD-14 | Avoir sur créance : montant autorisé, saldo recalculé serveur, historique conservé |
| CRUD-15 | Solaire : devis → vente → paiement comptant sans échéancier imposé |
| CRUD-16 | Solaire : devis → vente → échéancier optionnel → versement → solde serveur |
| CRUD-17 | Compteur/vente solaire/contrat actif sans endpoint : aucun faux bouton DELETE, aucune écriture non autorisée |
| CRUD-18 | Admin : créer utilisateur, changer rôle, révoquer compte, tester dernier administrateur et ancien token |
| CRUD-19 | Catalogue : nouveau brouillon → comparer → approuver → révoquer ; historique immuable |
| CRUD-20 | Journal d'audit consultable, interdiction de modification/suppression |
| CRUD-21 | Vérifier status utilisateur `REVOKED` / `DISABLED` entre UI, schéma et backend ; test de bout en bout |
| CRUD-22 | Permissions RBAC : UI affiche actions permises ; API rejette celles interdites malgré appel manuel |
| CRUD-23 | Whitelist Tauri : routes strictement acceptées, routes arbitraires / DELETE et URLs externes refusés |
| CRUD-24 | Aucune donnée/piece PDF/jeton de session réel dans logs, captures de test ou dépôts |
| CRUD-25 | Après annulation d'une facture, contrepassation ou avoir : situation et soldes restent cohérents et historiques visibles |

## 4. Cas de sécurité et non-régression

**NR-01** : aucune modification de facture historique lors d'un changement de taux/prix.  
**NR-02** : toute nouvelle facture utilité contient deux lignes eau et électricité ; eau=0 m³ ne disparaît pas.  
**NR-03** : QR documentaire interne uniquement, aucune prétention de certification DGI.  
**NR-04** : aucun fichier PDF serveur (disque, DB, blob, API, cache) ; PDF rendu exclusivement sur le poste.  
**NR-05** : montants, arrondis, devise et TVA calculés par Rust (`Decimal`), jamais par `number` JS pour la comptabilité.  
**NR-06** : ressources d'interface locales, mais aucune opération métier hors connexion dans le client actuel.  
**NR-07** : mécanisme SafeUpdate, signature et anti-rollback préservés ; aucun certificat fictif.  
**NR-08** : stabilité des migrations, identifiants, contrats historiques et permissions.

## 5. Commandes prévues (à adapter au dépôt/OS vérifié)

**Client React/TypeScript** :

```bash
cd v2/windows-app
npm ci
npm run build
```

**Backend Rust (workspace V2)** :

```bash
cd v2
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

**Windows** (poste Windows avec MSVC, Tauri CLI et dépendances requises) :

```powershell
cd v2/windows-app
npm ci
npm run build
npm run tauri -- build --bundles msi,nsis
```

Les commandes ci-dessus sont des **plans de validation**, pas des résultats déjà exécutés. Le `package.json` examiné ne comportait pas de script `test` UI : Codex doit choisir et intégrer un mécanisme de tests cohérent avec le dépôt avant d'affirmer une couverture frontend. Si PostgreSQL de test/Windows sont absents, ne pas marquer les cas correspondants PASS.

## 6. Exigences de preuve

- **À chaque incrément** : SHA, diff, commande et résultat, capture ou trace si test visuel/API, risque résiduel.
- **Aucun secret** : utiliser données synthétiques non nominatives, pas d'identités de clients, clés, tokens ou URL privés de production dans les captures.
- **Finance** : comparer les valeurs numériques structurées API, documents PDF rendus et soldes, pas seulement la présence visuelle d'un sigle de devise.
- **Windows** : enregistrer build Windows, OS/architecture, version WebView2, résultats installation/ouverture, fichier généré, empreinte et limites ; ne pas inventer une signature Microsoft.
- **Déploiement** : absent de cette mission. Si aucun test sur le poste cible SEL n'a eu lieu, verdict de recette matérielle `NON VALIDÉ`.

## 7. GO / NO-GO indépendants

- `UX` : GO seulement si navigation, dialogues, erreurs et a11y sont validés.
- `API` : GO seulement si droits, routes, transactions et tests requis sont prouvés.
- `FINANCE` : GO seulement si montants, snapshots, écritures correctives et PDF sont conformes.
- `WINDOWS` : GO seulement si app installée/testée sur environnement cible ou représentatif.
- `PRODUCTION` : **NO-GO par défaut** sans approbation indépendante du propriétaire et recette.

**Si un des gates P0 échoue, ne pas qualifier la mission de finie.** La PR technique peut être ouverte en DRAFT avec blockers, preuves et prochaines actions.
