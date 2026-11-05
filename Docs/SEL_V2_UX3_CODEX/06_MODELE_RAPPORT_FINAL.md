# 06 — Rapport final Codex (gabarit obligatoire)

> Ne remplacer les champs que par des **preuves réelles**. Aucun pourcentage de complétion estimé. Les états permis : `DONE`, `PARTIAL`, `NOT_STARTED`, `BLOCKED_API`, `BLOCKED_EXTERNAL`, `FAIL`.

## 1. Références d'exécution

- Dépôt :
- Branche de code :
- Base commit SHA + date :
- HEAD commit SHA :
- PR technique URL et numéro :
- PR concurrentes prises en compte :
- Figma page 16 (frames lus / non lus) :
- Figma page 17 (règles lues / non lues) :
- Environnement : OS / CPU / Node / Rust / Tauri / PostgreSQL de test :
- Date de contrôle :

## 2. Invariants protégés

| Invariant | Test / preuve | État |
|---|---|---|
| Facture combinée eau + électricité, eau 0 m³ visible | | |
| Calculs financiers et états par Rust / Decimal | | |
| PDF A4 généré localement, non stocké serveur | | |
| Snapshot, numéros et écritures financières historiques intacts | | |
| RBAC + whitelist Tauri + sessions respectés | | |
| Aucune opération réelle hors connexion annoncée | | |
| Aucune suppression destructive comptable | | |
| SafeUpdate et migrations non dégradés | | |

## 3. Parcours implémentés (renseigner chaque famille)

| Domaine / Figma | API réellement appelée | UI créée/optimisée | Tests | Statut |
|---|---|---|---|---|
| H00, navigation 6 domaines, S00 diagnostics | | | | |
| C01–C06, C30–C31 abonnés CRUD | | | | |
| O01–O03, C07–C10, C32 services et compteurs | | | | |
| O04–O05, C11–C12, C33 relevés | | | | |
| F05–F07, C13–C14 facture/annulation | | | | |
| F01–F04, C15–C17, C34 paiements/contrepassations | | | | |
| F08, C18–C19, C35 créances/avoirs | | | | |
| O06–O10, C20–C22, C28 solaire | | | | |
| A01–A09, C23–C25, C29 admin/catalogue | | | | |
| E01–E11, C26–C27 erreur/états | | | | |

## 4. Lacunes API

| Code | Route ou contrat absent | Figma concerné | Proposition | Tests à créer | État |
|---|---|---|---|---|---|
| B1 | Correction/remplacement compteur | C10 | | | |
| B2 | Rectification/annulation vente solaire | C22 | | | |
| B3 | Avenant/résiliation contrat actif | C28 | | | |
| B4 | Édition/abandon serveur de DRAFT, si nécessaire | C25/C29 | | | |

Pour chaque lacune : indiquer clairement si seul un écran d'information est livré, ou si une nouvelle API a été conçue, implémentée et vérifiée. Ne jamais déclarer un endpoint fictif prêt.

## 5. Exécution et preuves de tests

| ID / commande | Environnement | SHA | Résultat réel | Lien log/artefact |
|---|---|---|---|---|
| `npm ci` | | | | |
| `npm run build` | | | | |
| `cargo fmt --all -- --check` | | | | |
| `cargo clippy --workspace --all-targets -- -D warnings` | | | | |
| `cargo test --workspace` | | | | |
| Tests API PostgreSQL | | | | |
| Tests frontend navigation/CRUD | | | | |
| Test Tauri et packaging Windows | | | | |
| Comparaison Figma 1440×900 / 1280×720 | | | | |
| Génération/impression PDF A4 | | | | |
| Recette sur poste SEL (si disponible) | | | | |

## 6. Revue de diff et gestion de risques

- Fichiers source modifiés :
- Migrations ajoutées et justification :
- API modifiées et compatibilité :
- Contrats OpenAPI mis à jour :
- Tests de sécurité ajoutés :
- Effets sur V1 et anciennes branches :
- Rollback possible :
- Actions indisponibles explicitement désactivées :
- Risques bloquants :
- Corrections non réalisées et raisons :

## 7. Verdict indépendant

| Domaine | GO / NO-GO / NOT_TESTED | Preuve | Reste à faire |
|---|---|---|---|
| UX / navigation | | | |
| CRUD / correction | | | |
| Backend / API | | | |
| Montants / données / PDF | | | |
| Build Windows | | | |
| Installation / matériel cible | | | |
| Déploiement production | **NO-GO sans accord distinct** | | |

**Décision de fusion :** demander validation explicite avant fusion.  
**Décision de distribution :** demander validation distincte avant toute release ou déploiement.  
**Conclusion courte :** travaux prouvés / limitations / priorités restantes / liens vers commits, PR et tests.
