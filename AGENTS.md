# Instructions pour Codex — SEL Facturation Desktop

- Lis d'abord `Docs/CODEX_HANDOFF.md`, `README.md`, `db.py`, `domain.py`, `desktop.py`, `invoice_pdf.py`, `app.py` et la suite `tests/`.
- Le produit est un logiciel Windows **local** de facturation électrique SEL, indépendant de Monarque Connect. Ne pas remplacer la pile Python/Flask/SQLite sans approbation explicite.
- Protéger strictement les données et snapshots de factures ; migration additive uniquement, export JSON `sel-facturation-portable` v1 conservé.
- Ne jamais présenter le PDF comme authentifié par la DGI ; aucune signature/QR/identifiant fiscal fictif. Les tarifs/taxes réels doivent être validés par SEL.
- Respecter l'identité SEL et l'application en fenêtre native, sans ouverture systématique du navigateur.
- Corriger d'abord les défauts reproductibles ; ajouter les tests pertinents ; garder le périmètre compatible avec la livraison du 15/10/2026.
- Toutes modifications sur branche dédiée puis PR, CI Windows, et compte rendu. Vérifier tout build ou release avant de fournir ses liens.
- En cas de non-validation sur le matériel SEL, signaler clairement le statut **pilote non validé sur poste cible**.
