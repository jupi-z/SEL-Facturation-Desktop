# Facture PDF professionnelle — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Améliorer la lisibilité, la hiérarchie visuelle et la présentation A4 du décompte PDF SEL sans modifier les calculs, les snapshots ou les exports.

**Architecture:** Conserver ReportLab et `render_invoice()` comme moteur unique. Réorganiser la composition en blocs sobres (en-tête, client/compteur, relevé, détail tarifaire, synthèse financière, paiements, mentions et pied de page), avec des styles centralisés et des en-têtes répétés sur plusieurs pages.

**Tech Stack:** Python 3.12, ReportLab, pypdf, pytest, Poppler si disponible pour le rendu visuel.

**Spec:** `Docs/CODEX_HANDOFF.md`, section « Préparation métier » et critères PDF A4.

## Global Constraints

- Ne pas modifier le calcul, les taux, les assiettes, les snapshots SQLite ou l'export `sel-facturation-portable` v1.
- Ne créer aucune signature, QR, certification ou référence DGI fictive.
- Conserver l'identité SEL, le format A4, les informations existantes et la compatibilité des factures historiques.
- Utiliser uniquement des tirets ASCII dans les nouvelles chaînes PDF.
- Toute affirmation de qualité doit être appuyée par tests automatisés et inspection du rendu.

## Review Focus

- Facture sans lignes de consommation : conserver une sortie lisible et non cassée — test PDF minimal.
- Facture avec plusieurs lignes et paiements : empêcher chevauchement ou coupure de la synthèse — test de texte et pagination.
- Facture annulée : garder le marquage visible sans masquer le total — test de contenu.
- Champs entreprise/client vides ou longs : conserver des valeurs de repli et éviter les débordements — test de génération.
- Document multipage : répéter les en-têtes utiles et garder le pied de page avec numéro de page — test de pagination.

---

### Task 1: Stabiliser les attentes de rendu PDF

**Files:**
- Modify: `tests/test_storage_pdf.py`
- Test: `tests/test_storage_pdf.py`

**Interfaces:**
- Consumes: `invoice_pdf.render_invoice(invoice, payments)`.
- Produces: Assertions de présentation et de contenu qui échouent avant l'amélioration.

- [ ] **Step 1: Écrire les tests rouges** — vérifier le titre, les blocs financiers, le pied de page, l'absence de DGI/QR et la génération d'une facture multipage.
- [ ] **Step 2: Lancer les tests ciblés** — `pytest tests/test_storage_pdf.py -q`; constater l'échec des attentes nouvelles.
- [ ] **Step 3: Vérifier que l'échec vient de l'absence de présentation attendue**, et non d'un défaut de fixture.

### Task 2: Recomposer le document A4

**Files:**
- Modify: `invoice_pdf.py`

**Interfaces:**
- Consumes: mêmes snapshots JSON et mêmes paiements.
- Produces: mêmes octets sémantiques PDF avec composition visuelle améliorée.

- [ ] **Step 1: Centraliser les styles et couleurs** avec marges, hauteurs de ligne et espacements cohérents.
- [ ] **Step 2: Clarifier l'en-tête** avec identité SEL, libellé décompte, numéro et date, sans statut fiscal inventé.
- [ ] **Step 3: Améliorer les blocs client/compteur, relevé et lignes tarifaires** avec alignement stable, libellés homogènes et en-têtes répétés.
- [ ] **Step 4: Mettre en évidence le total, le paiement et le solde** tout en conservant les montants Decimal existants.
- [ ] **Step 5: Ajouter une fonction de pied de page sobre** avec numéro de document et pagination sur chaque page.
- [ ] **Step 6: Relancer le test ciblé** et obtenir le vert.

### Task 3: Inspection visuelle et régression complète

**Files:**
- Modify: `Docs/REVIEW_REPORT.md`

**Interfaces:**
- Consumes: PDF généré par les fixtures de test et résultat CI.
- Produces: preuve de rendu et statut de livraison mis à jour.

- [ ] **Step 1: Générer un PDF représentatif** avec facture, taxes, paiement et contenu multipage.
- [ ] **Step 2: Rendre les pages en PNG avec Poppler si disponible** et inspecter marges, contrastes, césures, tableaux, pied de page et absence de chevauchement.
- [ ] **Step 3: Exécuter toute la suite** — `pytest -q` — puis compiler l'EXE Windows.
- [ ] **Step 4: Mettre à jour le rapport** en distinguant clairement validation technique, validation PDF et recette encore non effectuée sur le poste/imprimante SEL.
- [ ] **Step 5: Commit, push et mise à jour de la PR** avec les preuves exactes.

