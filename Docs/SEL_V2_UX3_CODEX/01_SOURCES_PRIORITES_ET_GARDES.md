# 01 — Source de vérité et restrictions

## Date et repère

Passation établie le **6 novembre 2026** après examen de `main@109555533848d441444830748236f74714c2c8fd`. **Cet état n'est pas un verrou** : Codex doit re-vérifier le HEAD exact avant d'ouvrir sa branche de code.

## Ordre de préséance pour l'intégration

1. **Règles de sécurité, opérations et API en vigueur** : code du backend Rust, contraintes du client Tauri et tests actuels.
2. **Contrat actualisé** `v2/openapi/sel-v2.yaml`, dans la mesure où le code et les tests confirment l'implémentation ; signaler les contradictions.
3. **Produit distribué actuel** : `README.md`, `v2/README.md`, `v2/windows-app` ; la V2 actuelle est un client connecté.
4. **Parcours et rendu cibles** : page 16 Figma (76 écrans) et page 17 Figma (règles et recette), composants et assets Figma du même fichier.
5. **Contrat d'implémentation UX3** : ce dossier, quand compatible avec le code et les contraintes ci-dessus.
6. **Mandats historiques** : `AGENTS.md`, `CODEX_SEL_V2_BACKEND_START_HERE.md`, `Docs/SEL_V2_BACKEND/*`, dossiers V1 et anciennes pages Figma : toujours utiles pour la protection des données, mais leurs directives architecture/offline/front peuvent être **dépassées** par les décisions et le code les plus récents.

### Conflit historique express

- `AGENTS.md` décrit la V1 Python/Flask/SQLite ; cela **n'interdit plus** le travail autonome dans `v2/` sous mandat V2.
- Certaines spécifications du 2–3 novembre décrivent un client V2 offline-first avec SQLite/outbox et agent local. Le `README.md` et le client pris en charge sur `main` décrivent maintenant **un client Windows connecté, sans facturation hors ligne ni base métier locale**. **Ne pas ressusciter l'ancienne architecture** lors de la refonte UX.
- L'ancienne planification « backend d'abord, ne pas développer de nouveau frontend » était limitée à une phase antérieure. Le mandat actuel porte précisément sur l'intégration UX du client existant ; on ne reconstruit ni l'application depuis zéro ni le moteur financier en JS.
- Les anciens écrans Figma de synchronisation/offline peuvent rester visibles à titre historique. La **page 16** est la référence UX de cette mission. La vérité de fonctionnement reste le code de `main` à l'instant de l'implémentation.

## Fichiers techniques à ouvrir

```text
README.md
v2/README.md
v2/windows-app/package.json
v2/windows-app/src/App.tsx
v2/windows-app/src/App.css
v2/windows-app/src/api.ts
v2/windows-app/src/documents.ts
v2/windows-app/src-tauri/src/lib.rs
v2/crates/sel-api/src/lib.rs
v2/openapi/sel-v2.yaml
v2/migrations/postgres/
Docs/SEL_V2_BACKEND/IMPLEMENTATION_REPORT.md
.github/workflows/
```

Ne pas se limiter à cette liste : inventorier composants, tests, services et migration. Une route existe côté Rust **uniquement si** l'API et la whitelist Tauri autorisent bien la requête depuis ce client et si les permissions sont satisfaites.

## Vérifications prioritaires d'écart

- Les routes `GET/PATCH` clients et services impliquent `If-Match/ETag` et des refus de concurrence `409` ; vérifier leur traitement.
- La whitelist du transport Tauri (`src-tauri/src/lib.rs`) restreint strictement les chemins et méthodes ; **ne pas l'élargir globalement** pour « faire marcher » l'interface. Autoriser seulement les chemins prouvés, écrire tests de refus pour les autres.
- Les routes administratives sont dans le serveur et le frontend existant, mais certains sous-chemins peuvent nécessiter une vérification spécifique du transport.
- **Divergence à élucider** : valeurs de statut utilisateur `DISABLED` dans du code client versus `REVOKED` dans le schéma OpenAPI actuel ; inspecter le handler et les tests, choisir l'état correct sans dupliquer une mutation.
- Le client a une navigation React fondée sur `view` et des interactions groupées ; remplacer progressivement par un état de navigation typé sans casser les formulaires ni le serveur.
- Vérifier que l'authentification, les droits et les états de serveur sont cohérents avant d'implémenter les actions de correction.

## Garde-fous non négociables

- Jamais de suppression destructrice de facture, paiement, créance, historique de relevés, audit, snapshots ou identités de production.
- Aucune opération réelle sur SEL, aucun certificat, numéro fiscal ou taux inventé, aucune fusion ni déploiement sans feu vert distinct.
- Utiliser tests à données synthétiques et instances locales de test ; aucune réécriture des factures existantes, aucune modification des chiffres côté React.
- Ne pas contourner les droits pour permettre à « l'utilisateur unique » une action interdite par les rôles : les permissions viennent de Rust.
- Conserver le mécanisme de signature/contrôle d'intégrité et les protections contre rollback ; ne pas faire de changement de package/runtime injustifié.
- Distinguer « code présent », « compilé », « testé API », « testé sur Windows », « distribué » et « utilisé en production » dans les rapports.

## Non-observables

Une maquette reliée dans Figma n'est **ni** un test métier end-to-end **ni** une recette Windows. Un `cargo test` passant sans PostgreSQL ni Windows ne démontre pas la production. Signaler tout test indisponible comme `NON TESTÉ` ou `BLOCKED_EXTERNAL`.
