# 08 — Correctif prioritaire : création d'un kit dans le catalogue solaire

**Date de constat :** 11 novembre 2026  
**Statut :** spécification pour Codex, **non implémentée** par cette note  
**Priorité :** P1 correctif UX / validation métier (à traiter avant recette de la gestion des kits)  
**Périmètre :** client Windows React/Tauri V2, route de création de brouillon du catalogue solaire ; pas de mutation de données de production.

## 1. Signalement et reproduction

L'utilisateur tente d'enregistrer un brouillon de kit depuis **Administration → Référentiels → Catalogue solaire** avec les valeurs observées sur une capture :

| Champ | Valeur vue |
|---|---|
| Référence produit / SKU | `KIT SOLAIRE DE 12,5 kW` |
| Désignation / label | `KIT SOLAIRE DE 12,5 kW` |
| Prix de référence | `24819.77` |
| Devise | `USD` |
| Début d'effet | `01/10/2026` affiché dans le contrôle date (valeur HTTP attendue `2026-10-01`) |
| Fin d'effet | vide |

**Cause prouvée au niveau validation :** la référence SKU observée contient des **espaces et une virgule**, interdits dans le validateur Rust actuel. Si cette valeur est bien envoyée, elle est rejetée par la validation serveur.

**Ce qui n'est pas prouvé :** nous ne disposons pas de la réponse HTTP ni des journaux de **cette tentative précise** ; il peut exister d'autres erreurs concomitantes (droits, conflit de version/date, indisponibilité du serveur). Le SKU est néanmoins **certainement invalide** selon les règles inspectées. Ne jamais écrire dans un rapport que la réponse effective observée était `422` sans trace.

### Sources de code vérifiées (branche `main`, à réinspecter au HEAD de Codex)

1. `v2/crates/sel-storage/src/postgres.rs` — `create_solar_product_draft`, vers **lignes 1773–1805** : `sku.trim().to_ascii_uppercase()` puis contrôle `sku.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))`; limite **64 octets ASCII**, prix non négatif et jusqu'à six décimales, devise USD/CDF, période cohérente, en plus des contraintes d'idempotence.
2. Même fichier, vers **lignes 1839–1848** : conflit si combinaison `sku + effective_from` déjà présente. Aucune stratégie d'auto-renommage silencieux autorisée.
3. `v2/crates/sel-api/src/lib.rs` — `product_write_error`, vers **lignes 4009–4039** : `SOLAR_PRODUCT_FIELDS_INVALID` (HTTP 422) associe plusieurs erreurs de saisie à un message anglais générique ; `SOLAR_PRODUCT_PERIOD_OVERLAP` (HTTP 409), `IDEMPOTENCY_PAYLOAD_MISMATCH` (409).
4. `v2/openapi/sel-v2.yaml` — `CreateSolarProductDraftRequest`, vers **lignes 1127–1137** : schéma de création, `sku`, `label`, `price_reference`, `currency`, `effective_from`, `effective_to`. Le contrat OpenAPI doit être aligné sur les restrictions supplémentaires effectivement imposées par Rust.
5. `v2/windows-app/src/App.tsx` — `adminCatalogDefinitions.products` et `AdminCatalogPanel` / `createCatalogDraft` : contrôle React actuel sans validation explicite du motif autorisé de `sku` avant le POST. Les noms/positions ont pu évoluer sur les branches récentes ; rechercher ces symboles plutôt que supposer des lignes fixes.
6. `v2/windows-app/src/api.ts` — `centralAdminRequest` : transport et levée d'erreurs. Conserver les codes structurés de l'API pour présenter des messages adaptés sans divulguer de données sensibles.

## 2. Comportement utilisateur cible (obligatoire)

1. **Séparer clairement** `Référence produit (code interne)` et `Désignation commerciale`. La désignation peut garder les espaces, accents, virgules, unités et typographie française ; ne pas lui appliquer la restriction SKU.
2. Dans le champ SKU, indiquer avant saisie : **« Lettres A–Z, chiffres 0–9, tiret (-), tiret bas (_) ou point (.). Aucun espace ni virgule. »**. Limiter à 64 caractères ASCII. Utiliser une aide permanente et un message d'erreur associé au champ (`aria-describedby`).
3. **Proposer**, sans imposer, une transformation en code interne lisible depuis la désignation. Exemple du cas remonté : **`KIT-SOLAIRE-12-5KW`** ; la désignation reste `KIT SOLAIRE DE 12,5 kW`. L'utilisateur doit pouvoir modifier le SKU proposé ; **ne jamais le transformer silencieusement après validation utilisateur**. Le code proposé n'est pas une preuve d'unicité.
4. Avant l'envoi, valider `/^[A-Za-z0-9._-]{1,64}$/` (équivalent à la règle serveur pour ASCII) ; trim/uppercase explicites et sûrs ; détecter les espaces, caractères accentués, virgules, slash, guillemets, caractères Unicode non ASCII ; déplacer le focus vers le champ invalide.
5. Conserver la précision monétaire côté transport en chaîne décimale conforme à OpenAPI, valider `0 <= prix <= limite serveur` et **six décimales maximum** sans convertir à un flottant susceptible de perdre la valeur. `24819.77` est acceptable. Ne pas introduire de règle tarifaire nouvelle.
6. Date : afficher une date locale lisible tout en envoyant ISO `YYYY-MM-DD`; `effective_to` vide devient `null`; si fournie, elle ne peut précéder `effective_from`. Ne pas interdire une date historique sans règle serveur explicite.
7. Devise : liste valide `USD` / `CDF`, sans devise inventée.
8. Lors du clic **Enregistrer le kit** : garder la confirmation métier existante, désactiver la soumission en cours, ne pas envoyer une valeur manifestement invalide, afficher les erreurs **à proximité des champs** et préserver intégralement la saisie après un refus.
9. Après réponse **201/200** et rechargement : afficher le nouveau kit avec son état **DRAFT / brouillon**, **non vendable** jusqu'à approbation SEL ; ne jamais dire que le kit est déjà approuvé ou disponible à la vente.
10. Si le backend refuse : afficher un message français spécifique **sans masquer le code** dans les logs de diagnostic autorisés :
   - `422 / SOLAR_PRODUCT_FIELDS_INVALID` → « Les informations du kit sont invalides. Vérifiez la référence, le prix, la devise et les dates. » ; si le SKU est localement invalide, cibler prioritairement le champ SKU.
   - `409 / SOLAR_PRODUCT_PERIOD_OVERLAP` → « Une version de ce code produit existe déjà pour cette date d'effet. Consultez les versions avant de recommencer. »
   - `409 / IDEMPOTENCY_PAYLOAD_MISMATCH` → message de conflit sans régénérer arbitrairement une opération de paiement/écriture.
   - `401/403` → session/droits ; `503` ou réseau → indisponibilité/relance sans prétendre que le brouillon est créé.
   - Ne pas inférer automatiquement **quel** champ serveur a échoué quand la réponse est générale : dans ce cas présenter une erreur générale et conserver les champs.
11. Conserver **l'API Rust comme autorité**. Ajouter la validation côté client pour l'ergonomie mais ne pas supprimer ou affaiblir la validation serveur, ni contourner le RBAC ou la whitelist Tauri.

## 3. Changements demandés à Codex

### Frontend (obligatoire)
- Extraire une validation `validateSolarProductDraft` testable à partir du formulaire `AdminCatalogPanel` (`kind === "products"`), avec erreurs champ par champ ; l'intégrer à la séquence de soumission et à la présentation d'erreur.
- Éventuellement extraire un helper pur `suggestSolarSku(label)` ; documenter un traitement **déterministe** des accents / espaces / virgule, sans supposer unicité. Laisser l'utilisateur corriger le SKU.
- Normaliser les contrôles de prix et de dates pour l'API ; préserver les valeurs après échec. Réutiliser les primitives de formulaire existantes et éviter une deuxième implémentation indépendante.
- Localiser les erreurs du catalogue solaire de manière ciblée, et adapter `centralHttpError` / `humanError` si nécessaire sans casser les erreurs des autres modules.
- Conserver le workflow **DRAFT → approbation explicite**.

### Contrat backend et API (sans changement de règle métier)
- Conserver la validation restrictive Rust ; si utile, **compléter la documentation OpenAPI** de `sku` par `pattern: '^[A-Za-z0-9._-]{1,64}$'` et décrire les codes d'erreur.
- Améliorer le diagnostic serveur **uniquement** s'il peut distinguer de façon fiable les champs fautifs tout en conservant la compatibilité des codes d'erreur et les tests existants.
- Ne pas ajouter DELETE, mise à jour silencieuse de `sku`, migration destructive ou correctif direct de données SEL.

## 4. Tests d'acceptation automatisés (ajouter et exécuter)

| ID | Entrée | Attendu |
|---|---|---|
| SOL-SKU-01 | `KIT SOLAIRE DE 12,5 kW` comme SKU | Validation locale invalide avec explication en français ; **zéro POST** |
| SOL-SKU-02 | `KIT-SOLAIRE-12-5KW` comme SKU, même désignation | SKU validé localement, POST avec champ `sku` exact |
| SOL-SKU-03 | `ÉNERGIE-10KW` / `KIT/12` / `KIT 12` | Rejet local ; ne pas transformer silencieusement |
| SOL-SKU-04 | chaîne ASCII de 64 caractères | Acceptée côté format |
| SOL-SKU-05 | chaîne de 65 caractères | Rejet local |
| SOL-SKU-06 | saisie de la désignation `KIT SOLAIRE DE 12,5 kW` | Texte français conservé dans `label` |
| SOL-PRI-01 | `24819.77` USD | Prix conservé en chaîne décimale correcte |
| SOL-PRI-02 | prix négatif / sept décimales / valeur hors limite | Blocage local et explication |
| SOL-DATE-01 | début `2026-10-01`, fin vide | Payload ISO et `effective_to: null` |
| SOL-DATE-02 | fin antérieure au début | Blocage local |
| SOL-ERR-01 | réponse 422 `SOLAR_PRODUCT_FIELDS_INVALID` | Erreur française, champs conservés |
| SOL-ERR-02 | réponse 409 `SOLAR_PRODUCT_PERIOD_OVERLAP` | Message doublon date/SKU ; aucune seconde écriture |
| SOL-ERR-03 | 401/403/503, panne réseau | États distincts, saisie conservée et aucun faux succès |
| SOL-DRAFT-01 | création réussie (fixtures serveur/test PostgreSQL) | Kit DRAFT non vendable tant que non approuvé |
| SOL-DRAFT-02 | double clic / retry | Pas de doublon durable, idempotence intacte |
| SOL-REG-01 | tous les catalogues non-solaires | Pas de régression des formulaires tarifs, TVA, change et ligne communication |

**Ne pas exécuter de tests sur les données réelles**. Détailler `PASS / FAIL / NOT_RUN` avec logs et SHA. Si le test en PostgreSQL/Windows n'est pas disponible, noter précisément la limite ; ne pas inventer la réussite.

## 5. Livraison

1. Intégrer ce correctif dans **la branche technique d'implémentation en cours**, après inspection des travaux concurrents : `monarque/sel-v2-ux3-implementation-20261106` et PR #60 si encore actives ; vérifier le HEAD et l'état de fusion. **Ne pas écraser** les changements ultérieurs, ne pas reprendre une ancienne version de `App.tsx`.
2. Fournir un diff réduit, les tests correspondants et les preuves du cas remonté (réponse 422 reproduite en test, pas présentée comme observée en production).
3. Reporter les résultats dans `06_MODELE_RAPPORT_FINAL.md` ou le rapport d'implémentation réel.
4. Ne pas fusionner ni déployer sans validation explicite.
5. Si le client Windows déjà installé reste inchangé après ce développement, indiquer clairement qu'un nouveau build/installateur testé et approuvé sera nécessaire pour en bénéficier.

**Critère de clôture :** le SKU invalide du signalement est arrêté **avant** l'appel serveur avec une indication claire ; le SKU corrigé et les autres données valides permettent de créer **un brouillon**, pas une vente ou approbation automatique ; le backend conserve ses règles et la saisie n'est jamais perdue sur erreur.
