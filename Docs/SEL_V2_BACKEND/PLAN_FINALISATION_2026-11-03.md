# Plan de finalisation SEL V2 Backend — 2026-11-03

## Objectif

Fermer les lacunes techniquement réalisables de la branche `monarque/sel-v2-backend` sans développer de nouveau frontend, sans fusionner dans `main`, sans publier de release et sans toucher aux données réelles de SEL.

## Phases et critères de sortie

### P0 — Paiements et affectations durables

- Ajouter l’affectation post-enregistrement `POST /v1/payments/{id}/allocate`.
- Agréger les affectations avant validation, verrouiller paiement et créances dans un ordre déterministe, empêcher toute surallocation et garantir l’idempotence persistante.
- Publier l’audit et l’événement de synchronisation dans la même transaction PostgreSQL.
- Ajouter les tests API/PostgreSQL de rejeu, doublon, surallocation et concurrence.

### P0 — Synchronisation commerciale et reprise

- Vérifier chaque type d’événement financier et les projections SQLite : facture combinée, paiement, affectation, solaire, contrat, échéancier, correction.
- Corriger les pertes silencieuses et les statuts `LOCAL_PENDING`/`CENTRAL_ACCEPTED`.
- Ajouter les scénarios de reprise après commit central, avant accusé, redémarrage local et rejeu.

### P1 — Moteur financier complet

- Compléter remboursements, crédits, annulations et contre-écritures append-only.
- Compléter soldes échus, retards, paiements d’échéances et conflits de contrats.
- Vérifier eau + électricité, zéro m³, tarifs approuvés, devises et snapshots sans inventer de valeur métier.

### P1 — Sécurité, API et exploitation

- Compléter pagination/filtres, tests IDOR/RBAC et refus des configurations de production incomplètes.
- Vérifier les identifiants stables, appareils et grants hors ligne.
- Documenter les prérequis TLS, sauvegarde/restauration et clés de production comme validations externes lorsqu’elles ne sont pas exécutables ici.

### P1 — Migration, tests et preuve de livraison

- Exécuter toutes les validations réalisables avec les outils disponibles et distinguer PASS, FAIL, NOT_TESTED et BLOCKED_EXTERNAL.
- Mettre à jour `IMPLEMENTATION_REPORT.md` avec le dernier commit, la CI et les preuves exactes.
- Auditer les branches et PR avant toute proposition de suppression ; ne rien supprimer sans preuve qu’aucun travail utile n’est perdu.

## Règles de vérification

- TDD pour chaque correction : test de régression observable, implémentation minimale, tests ciblés puis suite complète.
- Aucun test supprimé ou ignoré pour obtenir une CI verte.
- Aucun stockage de PDF ou fichier de facture côté serveur.
- Les limites matérielles (toolchain Rust locale, PostgreSQL local, Windows SEL, clés de production, données V1) restent explicitement signalées.
