# Mission Codex — correction finale et validation SEL Facturation

**Date : 23 octobre 2026. Statut : cahier des charges impératif.**
**Point d’entrée : `CODEX_FINAL_GATE_START_HERE.md`.**
**Projet : `jupi-z/SEL-Facturation-Desktop`.**

## Objectif

Terminer et démontrer, avec preuves, la version **SEL Facturation eau + électricité**. Pour TOUT nouvel abonné et TOUTE nouvelle facture : UNE facture, les DEUX services (`ELECTRICITY` en kWh et `WATER` en m³), montant de chaque service et total commun ; 0 m³ reste **visible, à montant 0,00**, y compris en absence de tarifs d’eau approuvés. Une consommation d’eau positive exige les tarifs et règles fiscales réels **validés par SEL**. Aucune valeur métier fictive n’entre en production.

## État connu et politique de branches

- Base `main` lors de la rédaction : `d37ec5d1ec96b4e52c1d60ef0afe28788a6ab804`.
- **PR technique principale #47**, branche `monarque/water-electricity-crud`, HEAD audité : `df3be22e81068b4cbabf78afdcf92fd2cfd8bb87` ; CI Windows verte sur ce commit mais défauts fonctionnels P0 identifiés.
- PR #41 antérieure sur facture combinée; vérifier le recouvrement avec #47, ne pas la fusionner par accident. PR #46 gère une politique CI distincte, à vérifier, pas à fusionner automatiquement.
- `main` contient SafeUpdate V1 **sans clé publique de production**, la release publique reste `v0.3.29`, la `v0.3.31` est un brouillon sans asset.
- Ne jamais se fier aux SHA ci-dessus si des commits plus récents sont intervenus : **refaire le diff et le diagnostic au démarrage**. Les tests précédents ne constituent pas une preuve pour de nouveaux commits.

## Séquence obligatoire

1. Lire intégralement `AGENTS.md`, le présent dossier (tous les fichiers) et les spécifications antérieures encore pertinentes ; lire **code réel**, tests, CI, packaging et migrations.
2. Reproduire chaque défaut décrit par un test **rouge** avec la branche Codex actuelle, avant tout correctif. Si un défaut est déjà résolu, fournir SHA + test + preuve et le retirer du travail restant ; ne jamais réintroduire un bug pour « prouver » sa correction.
3. Proposer un plan ciblé et vérifier les dépendances avec la PR #47; corriger sur la **branche technique dédiée** déjà active si Codex la contrôle, sinon nouvelle branche issue du dernier HEAD pertinent. N'écraser aucune modification concurrente; ne pas travailler directement sur `main`.
4. Réaliser les corrections P0, puis P1 et P2 documentées sans refonte inutile. Réduire les migrations, éviter les duplications de calcul et préserver les API et exports existants.
5. Exécuter la matrice de tests et le pipeline Windows; générer des PDF synthétiques USD et CDF, exporter CSV/JSON, vérifier les totaux par service et la page imprimable, vérifier les bases existantes via copies de test.
6. Vérifier que les nouvelles factures ont toujours DEUX services, que l'ancien historique ne change pas, que chaque correction de paiement reste append-only et que l'updater demeure fail-closed en absence de clés de production.
7. Préparer une seule PR technique révisable, tests verts, rapport de preuves et checklist GO/NO-GO. **Ne pas fusionner, taguer, publier de release, modifier les secrets ni déployer chez SEL sans accord exprès du propriétaire.**

## Règles non négociables

- **Zéro changement silencieux de données historiques**; ne pas recréer/réinitialiser `sel.sqlite3`, numéros, paiements ou factures. Test de migration sur une copie seulement.
- Les montants sont tous en `Decimal` et une devise unique par facture; l'USD→CDF n'est appliqué **qu'une fois**, sans changer le montant ni la devise de stockage de la facture source.
- Aucune taxe/tarif/eau fictif, aucun QR DGI fictif, aucune affirmation de certification fiscale.
- La présence de la ligne d’eau sur une facture **ne constitue PAS une preuve de raccordement physique** : distinguer la règle de présentation de l'état réel du branchement.
- Le fait qu'une CI soit verte n'autorise pas le statut « prêt chez SEL » tant que les essais Windows matériels ne sont pas faits.
- Ne pas compromettre SafeUpdate ; ne jamais désactiver Ed25519 ni forcer `TRUSTED_UPDATE_KEYS` avec une clé de test comme si elle était de production.
- Les paquets non signés Authenticode restent admis selon la politique SafeUpdate; aucun contournement UAC, SmartScreen, Defender ou droits administrateur.
- Seules les exigences métier explicites de SEL peuvent déterminer les tarifs, taxes, taux et catégories. Pour l'eau positive, bloquer tant qu'elles sont inconnues.
- **Aucun résultat inventé** : chaque test et build signalés doivent avoir été réellement exécutés avec SHA exact, machine/runner, commandes, résultat et artefact reproductible.

## Sortie attendue de Codex

Un fichier `Docs/SEL_FINAL_RELEASE_GATE/IMPLEMENTATION_REPORT.md` sur la PR technique, comportant un tableau `EXIGENCE | FICHIER/CORRECTIF | TEST | PREUVE | STATUT`, les résultats du pipeline Windows, les PDF avant/après synthétiques, les restrictions restantes, la cohérence PR #41/#46/#47, et les critères explicites `GO revue PR / GO pilote SEL / NO-GO`.

Ne jamais annoncer 100 % si l'une des validations indispensables reste bloquée : indiquer « développement terminé, validation externe en attente ».
