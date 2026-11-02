# Audit des branches et PR — 2 novembre 2026

## Périmètre vérifié

- Dépôt : `jupi-z/SEL-Facturation-Desktop`
- `origin/main` au moment de l'audit : `09d9a244b6b243fccdf3313d0a9d1aace2daa071`
- Branche de travail : `monarque/sel-v2-backend`
- Tête locale et distante avant le correctif de formatage : `f0d0f955e78bc64d96ffb3836922039af54c0a09`
- PR cible : #58, ouverte, base `main`, mergeable selon l'état GitHub observé

L'audit croise les branches distantes, l'état de leurs PR et le contenu non présent dans `main`. Les branches ne sont pas supprimées sur la seule base d'une PR fusionnée : une branche fusionnée peut encore contenir un commit de version, de documentation ou de packaging non ancêtre de `main`.

## Décision de conservation

Les deux branches de référence à conserver sont :

1. `main`
2. `monarque/sel-v2-backend`

Les branches ci-dessous restent provisoirement conservées car elles portent des travaux ouverts ou des changements V1/Windows qui ne sont pas intégrés à `main` ni à la PR #58. Leur suppression sans consolidation ferait perdre une capacité utile ou rendrait impossible une revue ultérieure.

| Branche | PR | Décision | Motif |
|---|---:|---|---|
| `monarque/sel-v2-backend` | #58 | CONSERVER | branche cible de la mission V2 |
| `monarque/water-electricity-crud` | #47 | CONSERVER PROVISOIREMENT | moteur V1 eau + électricité et release gate non intégré à `main` |
| `monarque/combined-invoice-water-electricity` | #41 | CONSERVER PROVISOIREMENT | présentation PDF/QR et packaging V1 non intégré à `main` |
| `monarque/installer-webview2-resilience` | #54 | CONSERVER PROVISOIREMENT | résilience de l'installateur Windows, PR ouverte |
| `monarque/new-invoice-form-layout` | #55 | CONSERVER PROVISOIREMENT | parcours de saisie eau, PR ouverte |
| `monarque/enforce-main-ci` | #46 | CONSERVER PROVISOIREMENT | ajustement CI, PR ouverte |
| `monarque/sel-safeupdate-v1` | #43 fusionnée | CONSERVER PROVISOIREMENT | la tête contient encore des améliorations SafeUpdate non ancêtres de `main` |
| `monarque/invoice-presentation-client-notes` | #44 fusionnée | CONSERVER PROVISOIREMENT | changements de présentation et de packaging encore divergents |
| `monarque/ui-shell-polish` | — | CONSERVER PROVISOIREMENT | commits UI résiduels non ancêtres de `main` |
| `monarque/ux-first-use-forms` | — | CONSERVER PROVISOIREMENT | améliorations UX résiduelles non ancêtres de `main` |
| `monarque/client-detail-action-row` | — | CONSERVER PROVISOIREMENT | correction de fiche client non ancêtre de `main` |

Ces branches ne doivent être supprimées qu'après une PR de consolidation validée séparément. Cette mission V2 n'autorise pas à fusionner leurs changements UI/packaging dans `main` ou dans le backend.

## Branches rendues obsolètes

Les branches suivantes correspondent à des PR fermées, à des doublons documentaires ou à des essais de packaging déjà remplacés par des commits plus récents. La comparaison avec `main`, l'état de la PR et le contenu des branches ne montrent pas de fonctionnalité V2 indispensable à préserver :

- `docs/sel-final-go-no-go-audit-20261023` — PR #51 fermée, doublon de documentation de release gate ;
- `docs/sel-final-stabilization-codex-20261023` — PR #52 fermée, doublon de stabilisation V1 ;
- `monarque/currency-controls` — PR #13 fermée, ancien essai V1 remplacé par les contrôles actuels ;
- `monarque/installer-upgrade-safety` — PR #12 fermée, ancien packaging remplacé par SafeUpdate ;
- `monarque/production-hardening` — PR #7 fermée, ancien audit sans capacité requise par V2 ;
- `monarque/windows-msi-ci` — PR #6 fermée, ancien pipeline remplacé ;
- `monarque/windows-msi-installer` — PR #5 fermée, ancien installateur remplacé ;
- `monarque/audit-livraison-sel` — PR #4 fermée, ancien audit remplacé par les release gates ;
- `docs/sel-release-gate-consolidate-issue49-20261023` — PR #53 ouverte mais devenue redondante, le contenu normatif est désormais conservé dans les documents actuels ;
- `docs/sel-v2-backend-offline-api-contract-20261102` — PR #57 fusionnée, documentation déjà reprise dans la branche V2 actuelle.

## Branches déjà intégrées

Les PR fusionnées historiques de V1 (#1, #2, #8–#11, #14–#20, #22–#45 hors PR explicitement signalées ci-dessus) ont été conservées dans l'historique de GitHub et leurs branches n'apportent pas de tête de développement nécessaire au backend V2. Elles sont candidates à la suppression distante après vérification de leur statut fermé/fusionné.

## Actions autorisées et limites

- Fermer la PR #53 devenue obsolète et supprimer sa branche après conservation de ce rapport.
- Supprimer les branches fermées/fusionnées sans PR ouverte et sans contenu requis par V2, après relecture de leur statut.
- Ne pas supprimer `main`, `monarque/sel-v2-backend`, ni les branches provisoirement conservées ci-dessus.
- Ne pas fusionner `main`, ne pas publier de release et ne pas déployer.

Le dépôt ne peut donc pas être réduit immédiatement à deux branches sans intégrer ou archiver explicitement les travaux V1/Windows encore ouverts. Le nettoyage sûr commence par les branches obsolètes clairement identifiées ; les branches utiles restent visibles jusqu'à leur consolidation dédiée.
