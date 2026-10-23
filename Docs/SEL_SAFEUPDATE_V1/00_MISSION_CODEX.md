# Mission Codex — SEL SafeUpdate V1

**Date :** 22 octobre 2026  
**Dépôt :** https://github.com/jupi-z/SEL-Facturation-Desktop  
**Point de départ documenté :** main@d5f4eb2cccc76618281da41c2a3e6a3b3f74af03  
**Statut :** mandat technique pour implémentation future, pas une fonctionnalité déjà livrée.

## Décision explicite du propriétaire
SEL Facturation doit proposer puis exécuter une mise à jour en un clic **sans attendre l'obtention d'un certificat Authenticode Microsoft**. Les exécutables restent non signés au sens Windows durant cette phase. **Cela n'autorise en aucun cas à exécuter des téléchargements non authentifiés.** Une signature cryptographique Ed25519 **du manifeste exact**, vérifiée hors navigateur par une clé publique de confiance intégrée au logiciel, plus une empreinte SHA-256 du package, est exigée.

Aucune stratégie de suppression ou de contournement de UAC, SmartScreen, Smart App Control ou Defender. Une installation par utilisateur doit être le chemin nominal sans privilèges élevés ; si la politique Windows s'y oppose, afficher la situation et les options de réparation manuelle.

## Priorité et coexistence
Une PR de facturation eau + électricité (PR #41 au moment de la rédaction) est en cours. Ne pas la perturber. Repartir du main **réel au moment du début du travail** ; ne pas travailler dans la branche Codex facturation, ne pas fusionner soi-même ses changements. Les fichiers de facturation métier et la présentation A4 n'appartiennent pas à cette mission.

## Livrables attendus
1. Spécification implémentée : détection automatique **non bloquante**, notification discrète et page Mises à jour, consentement unique, téléchargement progressif avec annulation, validation cryptographique, sauvegarde pré-installation, fermeture propre, mise à niveau silencieuse per-user, relancement/diagnostic.
2. Module de vérification **Ed25519 sur octets exacts** du manifeste + SHA-256 des bytes de l'EXE Windows. Clé(s) publiques épinglée(s) au binaire, clé privée hors dépôt. Rejet complet de toute mise à jour non authentifiée, même si elle vient de GitHub et qu'elle est sur HTTPS.
3. Petit orchestrateur d'installation indépendant, embarqué dans la version initiale de SafeUpdate ou créé par le binaire déjà fiable ; ce composant attend la fermeture du processus, lance le chemin d'installation WiX supporté, contrôle le code retour et relance SEL si c'est possible.
4. Migration progressive depuis l'actuel updater v0.3.24 : les anciennes versions **ne comprennent pas** Ed25519 et refusent `signed:false`; la première bascule nécessite une installation de transition validée manuellement. Ne pas promettre une migration automatique depuis 0.3.24 sans preuve.
5. Scripts de release reproductibles qui préparent le manifeste puis le signent avec une **clé privée détenue par le propriétaire**. Aucun secret dans le code GitHub, artefact publié, log CI, documentation ou cache. Génération initiale de paire et procédure de sauvegarde/rotation.
6. CI Windows et tests adversariaux + recette réelle : install / upgrade / réparation, ancien Inno, version installée, signature invalide, hash faux, rupture réseau, coupure de courant simulée, récupération, SmartScreen/UAC, WebView2 utilisateur.
7. `Docs/SEL_SAFEUPDATE_V1/IMPLEMENTATION_REPORT.md` après intervention : matrices de tests, captures et journaux expurgés, risques résiduels, numéro du PR, statut GO/NO-GO.

## Méthode de réalisation
- Lire **tous les documents de ce dossier** dans l'ordre ; relire ensuite `AGENTS.md`, `CODEX_START_HERE.md` uniquement pour comprendre le travail parallèle, `update_client.py`, `update_policy.py`, `app.py` (routes updates), `desktop.py`, `static/desktop.js`, `templates/updates.html`, `db.py` (pre_update_backup), `installer_version.py`, `packaging/preflight_launcher.py`, `packaging/bundle/Bundle.wxs`, `packaging/wix/Product.wxs`, `.github/workflows/windows.yml` et tous les tests updates/packaging.
- Inventaire initial obligatoire : faits du dépôt / hypothèses / risques et plan de migration minimisant la surface de code.
- Développer par tâches courtes et tests rouges → verts, sur **une branche dédiée**, une PR **non fusionnée**.
- Autorisation : développer, corriger, tester et ouvrir PR. **Interdit de fusionner sur main, de révoquer/supprimer une version, de publier une release ou d'activer silencieusement un canal de mise à jour sur des postes SEL** avant revue du propriétaire.
- Aucun contournement de mécanismes de sécurité du système, aucune exécution arbitraire par une route Flask ; le pont pywebview natif ne doit lancer que le package intégralement validé d'une transaction de mise à jour acceptée.
- Ne jamais bloquer la facturation hors ligne ni modifier les factures déjà émises.

## Critère de fin
La mission n'est PAS terminée lorsqu'une maquette affiche 100 %. Elle est terminée lorsqu'une version **non signée Windows mais authentifiée via manifeste Ed25519**, installée per-user, passe les essais end-to-end **sur Windows** avec droits standard, protège SQLite, traite les interruptions sans perte métier, produit un rapport exact et ouvre une PR prête à réviser. L'absence de recette poste SEL doit être déclarée explicitement.
