# 04 — Plan d'implémentation progressif (Codex)

## Règles de reprise

**Ne pas recommencer l'application.** Stabiliser les composants et flux de `v2/windows-app`, refactoriser seulement où le gain UX, la sécurité ou les tests le justifient. Ne jamais faire une migration de données pour obtenir un changement purement visuel.

Avant toute modification :
- `git status --short`, `git branch -vv`, `git log --oneline -8` et comparaison au HEAD distant ;
- inspecter les PR ouvertes, notamment `monarque/sel-v2-backend` et les travaux SEL concurrentiels ;
- lire les instructions du repo et les contraintes du transport Tauri ; reproduire les défauts significatifs ;
- inventorier ce qui existe et fonctionne déjà (ne pas dupliquer `PaymentForm`, `MeterReadingsView`, `ServiceAccountsManager`, `SolarContractManager`, `AdminConsole`, `AdminCatalogPanel`, `CustomerStatement`, `ClientPdfPreview`, etc. — les noms sont des points de recherche, pas un ordre d'utiliser chaque composant tel quel).

Conserver un unique lot technique cohérent, avec commits thématiques et PR non fusionnée. Conserver les tests et chemins de lancement actuels.

## Carte des couches à inspecter

| Couche | Référence existante | Tâche |
|---|---|---|
| Entrée client | `v2/windows-app/src/App.tsx` | Décomposer l'énorme vue en pages et actions cohérentes ; réutiliser les composants métiers |
| Styles/tokens | `v2/windows-app/src/App.css`, assets | Réconcilier logo SVG, tokens et interface Figma, gérer 1440×900 + 1280×720 |
| Transport client | `v2/windows-app/src/api.ts` | Requêtes typées, erreurs, idempotence, ETag/If-Match, prévention doubles soumissions |
| Pont Tauri | `v2/windows-app/src-tauri/src/lib.rs` | Conserver whitelist fail-closed et permissions limitées par route/méthode, tests de refus |
| API métier | `v2/crates/sel-api/src/lib.rs` | Vérifier/compléter uniquement routes nécessaires et autorisées |
| Contrats | `v2/openapi/sel-v2.yaml` | Faire correspondre contrats réels / versions / statut, tests OpenAPI |
| Données | `v2/crates/sel-storage/`, migrations | **Ne pas** modifier sauf besoin métier réel justifié/testé |
| Documents | `v2/windows-app/src/documents.ts` | PDF côté client depuis JSON structuré immuable, sans stockage serveur |
| Tests / CI | tests existants, `.github/workflows/` | Restaurer/rendre plus couvrants, ne pas masquer une régression |

**Architecture cible raisonnable (proposition, à adapter à l'existant)** :

```text
src/
  app/            AppShell, domain navigation, route context, error boundary
  components/     SELButton, Field, Table, MoneyDisplay, Notice, ConfirmDialog, Breadcrumb
  features/
    customers/    list, detail, create, edit, archive, restore
    services/     service account, meter list, readings, correction
    billing/      two-service invoice, preview, issue, document, void
    payments/     journal, record, allocation, reversal, credit
    solar/        products, quote, sale, contract, installments
    admin/        users/roles, catalog drafts/approve/revoke, audit
  api/            typed transport, validation, idempotency, status parser
```

Cette arborescence n'est **pas** un contrat rigide : éviter toute réécriture gratuite. Ne rien substituer au moteur Rust.

## Incrément 0 — Audit et baseline (P0)

Livrables :
- état du dépôt, SHA, PR, CI, environnement, codes disponibles et fichier Figma lu ;
- matrice des 76 écrans `frame → parcours → état existant → composant/route → endpoint/méthode → rôle → écart → test` ;
- inventaire des bugs reproduits, notamment actions directes sans vérification, formulaires qui perdent leur contexte et statut d'utilisateur divergent ;
- captures de référence Figma et captures du client réel **si l'environnement Windows le permet** ;
- une branche de travail et une stratégie de rollback non destructive.

**Sortie :** aucune hypothèse sur une API absente ; pas d'écrasement d'une modification concurrente.

## Incrément 1 — Socle d'interface et navigation (P0)

- Menu principal en six domaines, entrée Accueil, diagnostic serveur secondaire, navigation hiérarchique.
- Sous-écrans et transitions conditionnées aux permissions (les droits réels restent serveur).
- Paramètres typés de navigation (`customerId`, `invoiceId`, `paymentId`, etc.), source/retour, filtres, modal.
- Confirmation de perte du brouillon si l'on quitte une saisie non enregistrée.
- États génériques : loading, empty, 401, 403, 409, 422, 503, panne réseau, configuration commerciale incomplète, session expirée, succès réel.
- Design SEL fidèle : logo vectoriel sans plaque blanche, composants réutilisables, densité flat, a11y et clavier.

**Sortie :** navigation fonctionnelle, écrans accessibles, retour contextualisé, aucune action API encore activée sans autorisation.

## Incrément 2 — Abonnés, services et compteurs (P0)

- Répertoire, recherche/pagination, détail, création, modification, archivage/restauration ; API GET/POST/PATCH, If-Match, 409.
- Dossier abonné avec liens directs vers facture, créance, services, relevés, paiement ; conserver le contexte.
- Gestion des services eau/électricité avec ou sans compteur ; mise à jour uniquement des champs autorisés, contrôle de version.
- Inventaire et création de compteur ; l'édition/remplacement/suppression doit rester bloquée si route absente.
- Tests de permission, filtre, brouillon, archive/restore, 409, retour.

**Sortie :** dossier complet et modifiable, pas de perte d'historique.

## Incrément 3 — Facture unique et correction (P0)

- Formulaire eau kWh? **NON** : électricité en **kWh**, eau en **m³** ; exactement deux lignes même si eau=0.
- Prévisualiser à l'aide du serveur Rust avant émission ; confirmer émission, gérer retry/idempotence sans doublons.
- Facture validée consultable, PDF client généré localement et imprimable ; historique et filtre.
- Annulation via `/invoices/{id}/void`, soumise au RBAC, conditions de règlement et réponse serveur ; pas de suppression.
- Affichage de la TVA et prix depuis version approuvée ; pas de constantes commerciales à l'écran comme source autoritative.
- Tests zéro eau, eau positive, coût kWh, montant long, taxe, QR interne non fiscal, PDF local, numéros immuables.

**Sortie :** parcours émettre → remettre au client → retrouver → annuler si autorisé, sans altération historique.

## Incrément 4 — Paiement, affectation, contrepassation et avoir (P0)

- Journal/filtre, nouveau versement, sélection client, créances, ventilation et contrôle serveur avant écriture.
- Émission avec clé d'idempotence ; allocations vers une ou plusieurs créances ; montant non affecté visible si API le supporte.
- Reçu et PDF local ; détail du versement et historique.
- Contrepassation via `/payments/{id}/reverse` (motif/montant et RBAC), totales ou partielles selon conditions Rust.
- Avoir de créance via `/receivables/{id}/credit` ; pas de modification directe d'un solde.
- Tests doubles clics, retries, 403/409, montants incompatibles, affectations inter-clients, devise, demi-paiement, crédit.

**Sortie :** journal exact et corrigible sans perdre l'audit.

## Incrément 5 — Relevés, solaire et administration (P1)

- Relevés avec compteur, index précédent, saisie, consommation calculée serveur, correction append-only et historique.
- Solaire : catalogue approuvé, devis, vente, créance, paiement comptant **ou** contrat avec échéancier selon choix ; aucune création forcée de contrat.
- Gestion administrateur : comptes, changement rôle/statut, protection dernier admin, versions des tarifs, TVA, FX, produits, approbation et révocation avec confirmation.
- Audit en lecture seule ; état serveur, configurations manquantes et permissions.
- Corriger divergence `DISABLED`/`REVOKED` en s'appuyant sur les réponses backend/test.
- Documenter et désactiver les actions backend absentes (B1/B2/B3/B4) ; ne pas inventer des mutations « invisibles ».

**Sortie :** parcours fiables de toutes les opérations backend existantes, lacunes signalées.

## Incrément 6 — Régression, Windows et PR (P0 pour livraison)

- Exécuter `npm ci`, `npm run build` dans `v2/windows-app`.
- Depuis le workspace Rust, exécuter `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Les tests nécessitant PostgreSQL/Windows non disponibles restent **NON TESTÉS**.
- Exécuter les tests UI et API ajoutés ; exécuter une vraie recette Windows/Tauri lorsque les dépendances et la machine sont disponibles.
- Contrôler les parcours au clavier, 1440×900, 1280×720, PDF A4, impression et périphérique cible si possible.
- Contrôler le diff, les permissions, la whitelist Tauri, les paramètres de sécurité, les données/numéros.
- Mettre à jour le rapport, pousser la branche de code, créer une PR **non fusionnée** ; fournir HEAD, diff, tests, limites, risques, preuves et verdict.

### Consigne de priorité

| Priorité | Exigences |
|---|---|
| **P0 avant tout GO** | Aucun montant JS, facture combinée, émission confirmée, paiement idempotent, correctifs financiers auditables, auth/RBAC, contexte navigable, 403/409 et aucun stockage PDF serveur |
| **P1 avant UI « complète »** | Tous les parcours existants intégrés : relevés, solaire, catalogue, utilisateurs, versioning, états, a11y |
| **P2 optionnel** | Raffinements d'animation, automatisations de capture, reporting complémentaire, optimisation de performances démontrée par mesure |
| **BLOCKED_API** | Compteur (édition/remplacement), vente solaire (annulation), contrat actif (avenant) en absence de contrat/implémentation testée |

## Interdictions de raccourci

- Ne jamais remplacer une validation Rust par un calcul JS.
- Ne pas créer un faux état « écrit en base » avant la réponse serveur.
- Ne pas cacher les problèmes derrière des fixtures ou un mock en production.
- Ne pas utiliser le backend V1 Flask pour des écritures V2.
- Ne pas forcer merge, installer ou publier.
- Ne pas fusionner deux versions de catalogues sans validation métier et test.
- Ne pas supprimer une migration, test existant, snapshot ou historique.
