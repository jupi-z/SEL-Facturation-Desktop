# Chaîne de release et installation per-user SafeUpdate

## Diagnostic du code existant (baseline 22/10/2026)
- `version.py` = 0.3.24; `updates/latest.json` déclare `signed:false`, `channel:preview`, URL GitHub Release, SHA-256.
- `update_policy.py` valide des champs/version/hash/HTTPS mais pas la signature cryptographique du manifeste.
- `update_client.py` vérifie SHA-256 **et Authenticode** via PowerShell; sans éditeur attendu, le téléchargement piloté est refusé.
- `app.py` : `/updates/status`, `/updates/check`, `/updates/download` déclenchent recherche, sauvegarde et téléchargement; `templates/updates.html` affiche l'absence de signature et bloque la préparation.
- `desktop.py` : `install_update` exige `SEL_UPDATE_EXPECTED_PUBLISHER` et `verify_authenticode`, lance le bootstrapper puis ferme la fenêtre. Ce pont natif doit devenir le **dernier vérificateur** d'un jeton d'installation généré après consentement, et non une interface qui lance n'importe quel nom/hash choisis par la page.
- WiX perUser actuel : `packaging/wix/Product.wxs` et `packaging/bundle/Bundle.wxs`; wrapper de précontrôle dans `packaging/preflight_launcher.py`; vérification versions/installations dans `installer_version.py`; backup dans `db.pre_update_backup()`.
- Workflow `.github/workflows/windows.yml` autorise les previews unsigned mais les libelle « acceptation manuelle »; doit dissocier `windows_code_signed=false` et `manifest_ed25519_verified=true`. Aucune nouvelle release ne doit être publiée par simple modification de texte d'un manifeste.

## Livrables CI / publication
- Ajouter un outil dédié, testable **hors réseau** : `tools/generate_signed_update.py` ou équivalent. À partir des artefacts Windows construits, calculer `size_bytes` et `sha256`, version de `version.py`, `package_type`, release URL **immuable**, canal validé et metadata; signer les octets finalisés du manifeste avec une clé Ed25519 **déjà provisionnée**. L'outil ne doit pas créer par défaut une nouvelle clé différente à chaque publication.
- Ajouter un vérificateur de publication public/CI qui utilise uniquement la **clé publique** et recalcul le hash/size de l'artefact approuvé; rejeter toute discordance. Le job d'upload ne nécessite pas le secret Ed25519 si les artefacts ont été pré-signés et contrôlés hors CI.
- Publier en deux phases : (A) créer release immutable + assets et vérifier téléchargeabilité / checksums; (B) publier manifeste+signature de canal comme **ensemble atomique**, après approbation explicite. Ne jamais annoncer une release incomplète.
- Mettre une politique GitHub de branches/protections pour éviter qu'un PR modifie les clés publiques pinées ou fichiers de release sans revue.
- Garder la possibilité future d'Authenticode optionnel et un canal stable/preview distinct. Pas d'exposition de clés privées en actions ou logs.
- Mettre à jour la documentation `Docs/WINDOWS_CODE_SIGNING.md` pour distinguer authentification interne (obligatoire maintenant) et signature Windows (souhaitable plus tard).
- Des paquets Windows non signés ne doivent pas porter l'indication « Windows verified », « Trusted Publisher » ou équivalent.

## Orchestration locale de mise à jour
- Définir états formels persistants : `idle → available → downloading → verifying → ready → backup → closing → installing → postcheck → completed`, plus `cancelled | blocked | failed | recovery_required`.
- Les étapes `downloading`/validation ne doivent pas immobiliser la thread UI/Flask. Utiliser une seule tâche contrôlée, threads bornés, limites de mémoire/temps et journalisation.
- Avant `closing`, empêcher la création de nouvelles factures, attendre opérations DB en cours, sauvegarder et vérifier; **ne pas fermer tant que la sauvegarde n'est pas bonne**.
- Helper installé localement avec la version déjà fiable : reçoit **un identifiant opaque de transaction** (pas shell/URL libre), récupère une déclaration de mise à jour **signée et vérifiée**, contrôle staging/file hash/version, attend fermeture de l'app puis démarre WiX via une liste d'arguments codée en dur. Ne pas utiliser `shell=True`, PowerShell d'élévation, tâches planifiées privilégiées ou désactivation SmartScreen.
- Gestion du mutex existant : l'app doit libérer son verrou et les handles SQLite avant que l'installateur ne tente le remplacement des fichiers.
- Éviter toute opération dans Program Files/HKLM : MSI per-user + WebView2 per-user si requis; le bundle peut parfois afficher une UI native ou un prompt Windows malgré les arguments silencieux.
- Après retour installateur : gestion explicite des codes success, reboot requis, annulation, échec, app bloquée, installer pas trouvé, version installée divergente. Tester le relancement sous la session utilisateur **sans administrateur**.
- Au redémarrage, vérifier `APP_VERSION`, compatibilité des migrations SQLite et intégrité. La sauvegarde préupdate est une assurance contre perte **mais pas la permission de recharger automatiquement une DB plus ancienne dans une version plus récente**.
- Aucune promesse de rollback automatique à moins qu'une matrice Windows prouve l'installation précédente restaurable et la compatibilité DB, sans violer le refus de downgrade.
- Conservation d'au moins un paquet « known-good » hors des fichiers temporaires purgés, avec contrôle d'intégrité, plus procédure de reprise hors ligne. Supprimer les staging ratés seulement une fois la situation stable.

## Migration depuis 0.3.24
**Problème d'amorçage :** cette version ne contient pas encore notre clé publique et attend Authenticode. On ne peut pas lui faire accepter automatiquement une release unsigned par simple changement de `latest.json`, sans modifier son mécanisme de confiance initial. 

La première version SafeUpdate exige donc **une mise à niveau initiale supervisée et approuvée sur le PC de SEL** (ou une politique de déploiement IT explicitement autorisée). Ensuite, les versions signées **au niveau manifeste** passent par le mécanisme intégré. Ne pas supprimer cette condition du produit ni communiquer une fonction auto-update rétroactive inexistante.

## Sémantique de version et UX
- `APP_VERSION` correspond au MSI, bootstrapper et release tag; refuser downgrade / même version / saut incompatible; aucun changement du numéro de version simplement pour des retouches documentaires.
- Côté Windows non signé, le paquet peut encore être averti/bloqué par SmartScreen ou Smart App Control. Le chemin `recovery_required` couvre explicitement cette condition et fournit une explication sans contourner les contrôles.
- Une indisponibilité GitHub, un hash invalide ou un test insuffisant **ne doit pas empêcher SEL de démarrer ou facturer**.
