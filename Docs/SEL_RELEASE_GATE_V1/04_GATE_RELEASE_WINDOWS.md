# Portes de validation — Windows, sécurité et déploiement

## Statuts indépendants (ne pas les confondre)
- **Code prêt pour revue** : tous tests unitaires/HTTP/PDF/crypto verts sur SHA précis, corrections P0, CI Windows réussie.
- **Installateur candidat** : build PyInstaller + MSI WiX + Burn, hash SHA-256 et taille établis, Win10/11 testé au moins dans un environnement Windows de validation propre. Les artefacts CI de tests ne sont pas des releases commerciales.
- **Pilote SEL** : validation par SEL des tarifs eau/m³, dates applicables, catégories, devise, taxes et éventuels frais fixes; vérification visuelle/financière avec exemples approuvés.
- **Livraison commerciale** : validation express du propriétaire + recette du poste réel (installation, réinstallation, conservation SQLite, fenêtre WebView2, PDF ouvert/imprimé, sauvegarde hors du disque) et absence P0.

## Windows (P0 pour livrer)
1. Vérifier qu'un exécutable peut réellement être lancé après installation Windows 10/11 x64 sous **compte standard**, et que son démarrage n'exige pas de privilèges admin permanents.
2. Installer l'upgrade **par utilisateur** sur une installation v0.3.29 de test et sur environnement propre. Préserver `%LOCALAPPDATA%\SEL Facturation\sel.sqlite3`, les PDF historiques, `session.key`, numérotation et paiements. Tester redémarrages et reprise après échec.
3. Tester la présence/absence de WebView2, le composant runtime authentique Microsoft, la signature de cet installeur tiers et les chemins où les politiques Windows peuvent bloquer un programme Authenticode non signé.
4. Imprimer (ou documenter comme non testé) le PDF A4 sur imprimante réelle; ne pas confondre « PDF généré » et « impression confirmée ».
5. Tester la protection contre anciennes installations Inno, double installation, downgrade, version identique et update d'un MSI WiX précédent.
6. CI obligatoire sur SHA final, y compris vérification de disponibilité des modules ReportLab barcode/QR dans le binaire gelé.

## SafeUpdate — contrainte indépendante
- Le mécanisme SafeUpdate V1 a été fusionné au socle mais `safeupdate_keys.py` avait encore `TRUSTED_UPDATE_KEYS = {}` au dernier audit. Sans **clé publique Ed25519 de production préalablement approuvée et épinglée au binaire**, l'auto-update en production est impossible par conception et doit rester en **fail-closed**.
- La signature Authenticode Windows n'est **pas obligatoire** dans le contrat utilisateur actuel. Toutefois, une mise à jour autonome unsigned doit être authentifiée avec Ed25519 + SHA256 du paquet selon `CODEX_SAFEUPDATE_START_HERE.md`, jamais seulement HTTPS/hash non signés.
- Ne pas fabriquer une clé de production, ne pas stocker la clé privée GitHub, ne pas supprimer la vérification de signature. Fournir seulement une procédure claire de provisionnement et un chemin E2E avec clés **de test** avant autorisation d'une vraie release.
- Les avertissements et blocages SmartScreen/Smart App Control/UAC ne sont pas contournables ni garantis absents; les documenter. Un paquet WebView2 signé Microsoft ne rend pas le programme SEL signé Microsoft.
- Ne pas publier/modifier `updates/latest.json` ni `latest.sig.json` de production, ni une nouvelle GitHub Release sans autorisation explicite du propriétaire.

## Fonctionnel métier
- Une facture unique par client/période, contenant les deux sections même si les volumes = 0.
- Tarifs de l'eau positifs et régime fiscal doivent provenir des validations réelles de SEL; aucune fixture de test dans base client.
- CDF doit afficher un total, des sous-totaux, des lignes et des paiements **de même unité monétaire** et réconciliables; QR interne sans confusion fiscale.
- Les anciens documents restent lisibles et immuables; sauvegarde SQLite testée; exports conservés.

## Matrice des preuves
Pour chaque ligne : `environnement exact | système et version | binaire/commit SHA | commande/gestes | résultat PASS/FAIL/BLOCKED/NOT TESTED | log/capture | limite`.

### GO / NO-GO
- **GO de revue de PR** après résolution P0, preuves automatisées et CI au SHA final.
- **GO pilote** seulement après tests de machine de validation et revue métier; demande de validation préalable.
- **GO commercial** seulement sur approbation explicite et recette PC SEL, barèmes approuvés et dossier fiscal honnête.
- **NO-GO livraison** si erreur de conversion, tarif inactif appliqué, nouveau document mono-service, factures historiques modifiées, perte SQLite, tests Windows absents ou incohérence PDF.
- **NO-GO auto-update production** tant que paire de confiance et canal de release signé Ed25519 ne sont pas provisionnés. Ce blocage ne signifie pas qu'un installeur manuel, dûment testé et approuvé, est impossible.
