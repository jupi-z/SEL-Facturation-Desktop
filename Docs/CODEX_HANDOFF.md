# SEL Facturation Desktop — passation Codex (14 octobre 2026)

## Mandat et échéance
Auditer **le projet actuellement livré** sur `main` puis corriger les défauts avérés qui bloqueraient la démonstration et l'utilisation locale de SEL le 15 octobre à 09h00, heure de Kolwezi (UTC+2). Ne pas lancer un nouveau produit, ni intégrer Monarque Connect à ce stade.

**Baseline auditée** : `main@0fbbfbcb7ada4ba00e29be7330315c9532594770` ; préversion distribuée : `v0.2.0-rc1`. [Build Windows réussi](https://github.com/jupi-z/SEL-Facturation-Desktop/actions/runs/38614705237) mais **aucune recette matérielle Windows et imprimante SEL n'est attestée**.

## Architecture réelle
- Python 3.12, Flask, Waitress/Werkzeug, pywebview + WebView2, ReportLab, SQLite ; installateur Inno Setup et EXE PyInstaller.
- Interface dans fenêtre Windows ; serveur HTTP éphémère lié à `127.0.0.1` seulement.
- Données persistantes : `%LOCALAPPDATA%\SEL Facturation\sel.sqlite3` ; scripts `db.py`, `desktop.py`, `domain.py`, `invoice_pdf.py`, `app.py`.
- Flux : créer client/compteur → paramétrer barème → index période → calcul progressif/taxes → décompte PDF A4 → historique/paiements → export CSV/JSON.
- Les données historiques de facture sont des snapshots immuables ; les exports JSON `sel-facturation-portable` schema_version 1 préparent la migration éventuelle vers Monarque Connect, **sans dépendance runtime**.
- SEL ne dispose pas à ce jour d'un dispositif fiscal DGI opérationnel. Conserver la distinction : PDF de consommation **non présenté comme normalisé/certifié** ; ne créer aucun QR fiscal, code d'authentification ou homologation fictif. La mention alarmiste a été retirée du PDF client à la demande du donneur d'ordre ; cela **ne confère aucune validité fiscale**.

## Priorités impératives — P0 (avant remise)
1. **Installation et démarrage** : tester l'installateur Windows 64 bits, WebView2 Runtime présent/absent, ouverture dans sa fenêtre propre, fermeture/redémarrage, unicité de l'instance, éventuel antivirus et raccourcis.
2. **Actions PDF** : vérifier `Voir`, `Enregistrer`, `Imprimer` sur le vrai Windows, avec et sans lecteur PDF associé/imprimante connectée. `os.startfile(..., 'print')` dépend de l'association Windows : fournir un repli explicite vers `Voir PDF` puis Ctrl+P si nécessaire. Vérifier que le fichier téléchargé est valide, lisible, sans URL externe et sans mention DGI trompeuse.
3. **Moteur de calcul** : contrôler 0 kWh, 1 kWh, 100, 100.5, 200, 201 kWh, coef >1, frais, taxes et chaque assiette ; l'exemple 450→650 avec 300 puis 320 CDF/kWh donne 62 000 CDF HT et **72 540 CDF** avec taxe locale 1% et TVA 16% sur énergie seule. La capture client affiche 72 559,20 CDF ; l'écart doit rester signalé dans les notes internes, sans forcer un résultat inexpliqué.
4. **Intégrité** : vérifier unicité `(client_id, billing_period)` pour les factures actives, transactions, concurrence, paiement partiel et dépassement de solde, annulation sans paiement, documents historiques immuables.
5. **Pérennité** : migration/upgrade à partir des données d'une précédente installation, sauvegarde SQLite consistante, export JSON/CSV sans identifiants de session, restauration TESTÉE sur une copie. Aucune opération ne doit effacer `%LOCALAPPDATA%\SEL Facturation`.
6. **Préparation métier** : champs entreprise/abonnés, décompte A4, affichage des taux et des bases, mention légales qui sont effectivement connues, validation obligatoire des vrais tarifs et taxes par SEL ; aucune donnée fictive présentée comme officielle.

## Vérifications P1 ciblées / risques identifiés
- **Versionnement de l'installateur** : première préversion `v0.1.0-rc1` portait `AppVersion=1.0.0`, mais la suivante `v0.2.0-rc1` porte `AppVersion=0.2.0`. Corriger la progression des versions d'installation lors d'une **nouvelle release** sans écraser une archive déjà publiée, et tester l'upgrade.
- **Fichiers PDF temporaires** : `desktop.py` stocke des copies imprimées sous `documents-temporaires` sans politique de purge ; choisir une rétention sûre compatible avec le lecteur PDF et le secret des données abonnés.
- **Sauvegardes automatiques** : une copie par date, au démarrage, rétention 14 copies ; vérifier la couverture des opérations effectuées après la première ouverture du jour, clarifier la stratégie de restauration et de sauvegarde hors poste.
- **Historique & reporting** : le tableau de bord utilise `CAST(... AS REAL)` pour agréger les montants ; éviter que cette approximation soit prise pour un total financier faisant foi.
- **Références DGI** : la clé `footnote` héritée dans `db.py` et les exports n'est pas utilisée pour le PDF ; supprimer/nettoyer les mentions inutiles si besoin sans altérer l'historique.
- **Permissions** : un seul compte administrateur dans cette V1 ; ne pas prétendre à un journal multi-agent inexistant.
- **Sécurité de la fenêtre locale** : origine `127.0.0.1`, CSRF, sessions, CSP, pont `window.pywebview.api` et téléchargements ; refuser navigation externe malveillante / exposition réseau / traversal.
- **Test d'upgrade logiciel** : sauvegarder, installer version supérieure en conservant le même `AppId` Inno Setup, vérifier que la DB et les PDF anciens demeurent identiques.

## Contraintes de correction
- Priorité aux correctifs minimaux, additive-only pour SQLite ; ne pas recréer une application, ni transformer Flask en client web distant.
- Aucune suppression d'abonnés, de factures, de paiements ou de snapshots historiques ; pas de reset implicite de DB.
- Ne pas transformer la V1 en ERP/Monarque Connect ; préserver l'export `sel-facturation-portable` version 1 ; toute extension de format doit être versionnée et testée.
- Garder l'identité visuelle SEL et les documents clients sobres. Ne pas prétendre à la conformité DGI avant confirmation ; ne pas supprimer des informations fiscales réellement obligatoires par simple préférence esthétique.
- Les corrections passent par **branche dédiée → PR → CI Windows verte → fusion contrôlée**. Eviter les changements directement sur `main`. Aucun nouveau binaire annoncé sans build/release confirmé. Le bootstrapper WiX est le seul installeur de livraison supporté ; Inno Setup est historique et ne doit pas être utilisé pour les nouvelles installations.
- Ne pas écraser/supprimer la préversion `v0.1.0-rc1` tant que la nouvelle n'a pas passé une recette sur le PC cible ; elle constitue un recours technique, pas la version recommandée.
- Exposer les limites, les hypothèses et les blocages au lieu de maquiller le statut de la production.

## Exécuter la vérification
```powershell
py -3.12 -m venv .venv
.\.venv\Scripts\python.exe -m pip install -r requirements-build.txt pytest
.\.venv\Scripts\python.exe -m pytest -q
powershell -ExecutionPolicy Bypass -File .\build-windows.ps1
```
La CI : `.github/workflows/windows.yml`.

## Critères d'acceptation
- Les tests backend/PDF/HTTP/desktop passent et la CI Windows compile l'EXE et l'installateur.
- Recette explicitement attestée (ou marquée **non effectuée**) sur Windows avec installation, premier login, création client, facture 200 kWh, 2e facture, doublon rejeté, paiement partiel, sauvegarde/export, fermeture/redémarrage, facture PDF ouvert/imprimé.
- Aucun inventaire fictif de conformité ; tarifs/taxes/numéro fiscal transmis par SEL ou marqués « à confirmer » dans la documentation.
- Version de l'installation, changelog, release notes et chemins de téléchargement cohérents.
- Un rapport final `Docs/REVIEW_REPORT.md` cite les anomalies reproductibles, leur sévérité, les PR/commits, les tests passés, ce qui reste non vérifié, et la recommandation go/no-go.

## Pour Codex
Lis ce fichier, le README, tous les scripts et les tests. Commence par un audit **sans modifications**, rédige un plan minimal classé P0/P1, puis corrige les P0 confirmés sur branche dédiée. Vérifie et rends compte au lieu de faire des modifications spéculatives.
