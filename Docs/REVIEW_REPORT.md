# Rapport d'audit — SEL Facturation Desktop

Date : 15 octobre 2026
Branche : `monarque/ui-simplification`
Issue : [#3](https://github.com/jupi-z/SEL-Facturation-Desktop/issues/3)
PR : [#7](https://github.com/jupi-z/SEL-Facturation-Desktop/pull/7)
Correctifs principaux : `298b667`, `f5bd24b`, `0269b6c`, `4926c77`, `22f71ac`, `db09134` et `cf6e133` ; correctifs de préparation intensive en cours sur cette branche.

## Périmètre et méthode

Audit de `AGENTS.md`, `Docs/CODEX_HANDOFF.md`, `README.md`, des modules Python principaux, des templates/static, de la suite `tests/`, du workflow Windows et de l'installateur Inno Setup. Les défauts corrigés ont d'abord été reproduits par tests de régression en échec, puis vérifiés après correction.

## Anomalies confirmées et corrections

### P0 — impression Windows sans repli explicite

`desktop.py` appelait directement `os.startfile(path, "print")`. Si l'association Windows ne fournit pas le verbe `print`, l'action échouait sans ouvrir le document. Le pont ouvre maintenant le PDF dans le lecteur associé et renvoie un état `fallback=open` avec l'instruction Ctrl+P ; `static/desktop.js` l'affiche comme une aide, pas comme une erreur trompeuse.

### P1 — sauvegarde journalière obsolète

`db.automatic_backup()` conservait le fichier du jour sans le rafraîchir. Une base modifiée après le premier démarrage pouvait donc ne pas être incluse dans la sauvegarde quotidienne suivante. Le snapshot du jour est désormais régénéré à chaque démarrage/appel de la routine, avec le même emplacement et sans suppression de données métier.

La régénération est écrite dans un fichier temporaire, vérifiée par `PRAGMA integrity_check`, puis remplacée atomiquement ; l'ancien snapshot reste disponible si la copie échoue.

### P1 — version de santé incohérente

`/health` annonçait une version différente de la préversion et de l’installateur. La version applicative est maintenant centralisée dans `version.py` (`0.3.0`) et vérifiée par test avec l’installateur Inno Setup et les artefacts WiX.

### P1 — handles SQLite conservés pendant initialisation/sauvegarde Windows

Les blocs de contexte SQLite validaient les transactions mais ne fermaient pas explicitement les connexions. Sur Windows, cela pouvait maintenir un verrou sur `sel.sqlite3` et empêcher le remplacement/restauration d'un snapshot. `initialize()` et `backup()` ferment maintenant leurs connexions dans des blocs `finally`, avec un test de remplacement de la base après sauvegarde.

### P0 — fenêtre WebView2 « Ne répond pas » au démarrage

`DesktopDocuments` exposait la fenêtre native sous l'attribut public `window` dans l'objet passé à `js_api`. pywebview parcourait alors cet attribut et descendait récursivement dans l'objet WinForms `AccessibilityObject`, jusqu'à l'erreur `maximum recursion depth exceeded` et au blocage de la fenêtre. La référence est maintenant privée (`_window`) afin qu'elle ne soit pas exposée au pont JavaScript.

### P1 — présentation PDF insuffisamment structurée

Le PDF était lisible mais sa hiérarchie financière était trop discrète pour une remise professionnelle : en-têtes de tableaux peu contrastés, total et situation du compte séparés uniquement par du texte courant, et informations de règlement peu visibles. Le moteur ReportLab conserve désormais le même format de données, les mêmes calculs et les mêmes snapshots, tout en ajoutant une structure A4 plus nette : titre du document compact et aligné, en-têtes bleus contrastés, lignes alternées, total mis en évidence, blocs « CONDITIONS DE RÈGLEMENT » et « SITUATION DU COMPTE », échéance, montant déjà réglé et « MONTANT À RÉGLER ». Aucun élément DGI fictif n'a été ajouté.

### P1 — parcours utilisateur trop technique

Le shell d'authentification utilise maintenant une composition SEL dédiée sans barre latérale vide ni carte autour du formulaire. La navigation connectée expose un vocabulaire stable et un état actif accessible. Le tableau de bord distingue les montants émis, encaissés et restant à suivre, et signale la configuration du barème ainsi que la dernière sauvegarde détectée. Les formulaires abonnés/paramètres sont regroupés par tâche avec labels explicites. Enfin, l'émission d'un décompte passe par une prévisualisation serveur avant création immuable ; une nouvelle saisie invalide efface tout brouillon précédent.

## Vérifications réalisées

### Préparation à l'exploitation intensive

- Les agrégations du tableau de bord et de l'historique recomposent désormais les montants en `Decimal` au lieu de `CAST(... AS REAL)` ; le format portable v1 et les snapshots persistés sont inchangés.
- Les paiements historiques sont protégés par des triggers SQLite contre modification et suppression directes. L'édition d'un client applique aussi les limites de longueur déjà présentes à la création.
- Le workflow Windows sépare build et publication, réduit les permissions par défaut à `contents: read`, exige une publication manuelle et refuse d'écraser une release existante.

- Suite complète : `60 passed` avec Python intégré 3.12 et dans l'environnement de build isolé.
- Tests UI/parcours : shell auth, navigation active, tableau de bord, formulaires, prévisualisation/confirmation de décompte, états de paiement, actions PDF et pages d'erreur.
- Compilation PyInstaller locale après les évolutions UI : `SEL-Facturation-ui.exe`, build terminé avec l'icône SEL embarquée.
- `git diff --check` : aucune erreur de whitespace.
- Compilation PyInstaller locale : `Build complete!`, `dist/SEL-Facturation.exe` créé (23,8 Mo au dernier build) ; l'analyse a émis des avertissements de collecte facultative WebView2/pycparser à confirmer dans la CI Windows.
- CI Windows GitHub Actions : run final `38641344478` réussi ; tests, dépendances desktop, EXE, Inno Setup, MSI WiX et bootstrapper WebView2 validés sur `windows-latest`. La publication/upload de release est volontairement ignorée sur une PR brouillon.
- Tests de régression dédiés : sauvegarde rafraîchie, repli impression, message JavaScript et version `/health`.
- Restauration contrôlée : sauvegarde SQLite, remplacement de la base, réouverture et `PRAGMA integrity_check` validés localement.
- Test de restauration sur copie : snapshot copié vers une base de test, facture/snapshot retrouvés et `PRAGMA integrity_check` à `ok`, sans modifier la base live.
- Vérification statique : loopback `127.0.0.1`, CSRF, cookies HttpOnly/SameSite, CSP, requêtes paramétrées, exports sans clé de session/mot de passe, snapshots protégés par trigger SQLite, calculs métier en `Decimal`.
- Un premier smoke test avec inspecteur UI avait déclenché des erreurs WebView2 ; après le correctif `_window`, le test natif a été repris sans inspecteur et le parcours complet a été exécuté proprement sur un dossier de données temporaire.
- Les PDF temporaires de prévisualisation sont maintenant purgés au-delà de 7 jours, sans toucher aux PDF récents ni aux factures SQLite.
- Smoke test EXE après reconstruction : processus Windows répondant après 12 secondes et base créée dans un dossier de données isolé.
- Smoke test de fermeture : parent et processus GUI se ferment proprement via `CloseMainWindow()`, sans processus SEL-Facturation résiduel.
- Inspection visuelle du PDF A4 rendu en PNG : en-tête SEL, informations client/compteur, relevé, tableaux tarifaires, total, règlement, mentions et pied de page alignés sans chevauchement sur le cas représentatif 200 kWh avec paiement partiel.
- Test natif E2E sur EXE reconstruit : configuration, connexion, client, facture 450 → 650, total 72 540 CDF, sauvegarde PDF, paiement complet et fermeture propre ; journal applicatif sans `ERROR`, `Traceback` ou récursion.
- Artefacts E2E vérifiés : PDF commençant par `%PDF-`, export JSON `sel-facturation-portable` schema 1 et sauvegarde SQLite créée.

## Limites et non-validations

- Le lanceur `py -3.12` n'est pas installé dans l'environnement local. La suite a été exécutée avec le runtime Python 3.12 intégré à Codex ; le script `build-windows.ps1` n'a donc pas été exécuté tel quel.
- Inno Setup n'est pas installé dans l'environnement local ; l'installateur n'a pas été compilé ici.
- Aucune validation physique n'a été possible avec l'imprimante SEL, l'antivirus ou l'installateur Inno Setup. Le statut reste **pilote non validé sur poste cible**.
- L'inspection UI automatisée précédente avait elle-même déclenché la récursion WebView2 ; le correctif est maintenant testé sans inspecteur, avec démarrage et fermeture propre de l'EXE.
- La release n'a pas été publiée par la PR ; aucun nouveau binaire ni lien de release n'est annoncé ici.
- La CI Windows du commit final `91e499e` est verte : run `38641344478`, tests, dépendances desktop, PyInstaller, Inno Setup, MSI et bootstrapper réussis ; publication et upload d'artefacts volontairement ignorés sur une validation standard.
- La run `38634986894` a validé tests, dépendances desktop, EXE et Inno Setup, puis a échoué seulement sur l'upload d'artefact GitHub (`Artifact storage quota has been hit`). Le workflow limite désormais cet upload à une publication manuelle explicite ; la validation CI standard ne dépend plus du quota d'artefacts.
- Les tarifs, taxes, bases et identifiants d'entreprise restent à confirmer par SEL ; aucun statut DGI, QR ou identifiant fiscal fictif n'est ajouté.

## Avis de livraison

**NO-GO pour remise commerciale immédiate.** La CI et le packaging sont maintenant verts, mais le statut reste **pilote non validé sur poste cible** : il manque l'installation/exécution du bootstrapper sur le PC Windows 10/11 SEL, la recette avec imprimante/PDF par défaut, la restauration sur copie et la validation SEL des tarifs, taxes et identifiants d'entreprise. Aucun GO commercial ne doit être annoncé avant ces preuves.

### Incident de démarrage 0.3.28 — module Code 128 non inclus dans PyInstaller

- Le journal `application.log` du poste concerné montre un échec avant toute initialisation de fenêtre : `ModuleNotFoundError: reportlab.graphics.barcode.code128`, déclenché pendant l'import de `invoice_pdf.py`. Le message natif antérieur conseillait à tort de vérifier WebView2 ; le Runtime n'était pas la cause de cet incident.
- La collecte PyInstaller de la version 0.3.28 embarquait `webview`, mais pas les imports dynamiques de ReportLab utilisés par son registre de codes-barres. Le défaut se reproduisait même avec WebView2 installé.
- Les commandes de build local, le workflow Windows et le fichier `.spec` collectent désormais tout le paquet ReportLab. Le diagnostic startup indique le nom exact d'un module Python manquant et ne mentionne WebView2 que lorsque l'exception l'identifie réellement.
- Un mode `--sel-packaging-smoke`, sans fenêtre et sans accès à SQLite, importe l'application réelle et `reportlab.graphics.barcode.code128` depuis l'EXE gelé. Le build local 0.3.29 et ce smoke test passent ; la suite compte 262 tests passants.
- La version est incrémentée en **0.3.29** pour assurer le chemin de mise à niveau. Le build local réussit pour l'EXE, le MSI et le bootstrapper ; Authenticode reste `NotSigned`. La recette d'installation et de lancement sur le poste cible SEL demeure à effectuer avant de déclarer le pilote validé.

## Mise à jour métier du 16 octobre 2026

La branche `monarque/consumption-tariffs` ajoute le modèle de facturation confirmé par SEL : Résidentiel 0,095 USD/kWh HT, Commercial 0,12 USD/kWh HT, TVA 16 %, aucune taxe locale pour les nouvelles factures, émission mensuelle en USD, conversion CDF optionnelle au taux configurable initial de 2 360. La consommation directe est disponible sans compteur ; les relevés restent un mode futur optionnel. Les anciennes données et snapshots restent compatibles.

Validation locale de la branche : 82 tests exécutés par groupes, tous passants ; EXE PyInstaller produit et démarré dans un dossier de données isolé. L'installateur WiX/Inno Setup, l'imprimante SEL et le poste cible restent à valider séparément.

### Addendum de validation — 16 octobre 2026

- La branche contient en plus la correction du tableau de bord qui évite de masquer des factures USD lorsqu'une ancienne configuration conserve CDF comme devise d'affichage. Régression dédiée ajoutée.
- Suite actuelle : **83 tests collectés, 83 passants**, exécutés en groupes bornés sous Python 3.12 ; `git diff --check` est propre.
- PyInstaller a produit `dist/SEL-Facturation.exe` (23 384 724 octets). Avec un dossier `SEL_DATA_DIR` isolé, le smoke test de l'EXE reste répondant pendant huit secondes et crée `sel.sqlite3`, `session.key`, `application.log` et `sauvegardes` ; ce test ne remplace pas une recette graphique sur le poste SEL.
- Le MSI WiX a été recompilé et son extraction administrative a réussi (code 0, EXE retrouvé dans l'arborescence extraite). Le bootstrapper WiX avec l'installeur autonome WebView2 a été recompilé sans avertissement après ajout de la détection du runtime.
- Inno Setup n'est pas installé localement ; aucun installeur Inno n'est donc déclaré validé. Aucun test d'installation réel ne doit être annoncé tant que le bootstrapper n'est pas exécuté sur un Windows 10/11 propre et que la conservation/mise à jour des données n'est pas constatée.

### Addendum installation et mise à jour — 16 octobre 2026

- L'audit a confirmé que la coexistence Inno Setup/WiX pouvait produire des comportements différents selon le fichier lancé. WiX Burn devient maintenant le seul chemin de livraison supporté ; Inno est explicitement marqué legacy et n'est plus compilé ni publié par le workflow Windows.
- `build-windows.ps1` construit désormais l'EXE, le MSI et le bootstrapper WiX avec WebView2, et échoue explicitement si WiX 4 est absent au lieu de produire un paquet incomplet.
- La politique `update_policy.py` accepte uniquement un manifest HTTPS pour un bootstrapper, avec SHA-256 valide et version strictement supérieure ; elle refuse les versions identiques, les rétrogradations, les URL HTTP et les packages non supportés. Cette politique ne télécharge ni n'exécute de package depuis une requête web.
- La séparation des données reste inchangée : les mises à jour remplacent les programmes, jamais `%LOCALAPPDATA%\\SEL Facturation`. Une recette Windows doit encore confirmer le comportement exact du bootstrapper installé sur un profil propre et la migration depuis une ancienne installation Inno.
- La protection pré-migration est maintenant exécutée avant `db.initialize()` pour toute base existante ; un snapshot `sel-preupdate-*.sqlite3` est conservé avant les migrations additives.
- Le client de mise à jour valide désormais la taille du manifest/package, l'HTTPS final, la version sémantique, le produit, le type de package, le SHA-256 et exige une signature Authenticode du publisher attendu avant qu'un package puisse être remis à l'installeur. L'endpoint `/updates/status` ne télécharge ni n'exécute de fichier.
- Validation actuelle : **99 tests passants**, compilation Python, smoke EXE répondant avec base et sauvegarde isolées, compilation MSI/bootstrapper et extraction MSI réussies.

### Addendum interface Paramètres — 17 octobre 2026

- La page **Paramètres SEL** est passée d’un affichage simultané de quatre blocs et de quatre modales à une navigation par onglets : Vue d’ensemble, Identité SEL, Barème et taxes, Types d’abonnés et Politique.
- Les formulaires sont désormais visibles dans leur contexte, sans modale, avec une synthèse compacte et sans effet de carte. L’onglet actif est conservé après enregistrement via une URL `?tab=...#...`.
- Les onglets implémentent `role=tablist`, `role=tab`, `role=tabpanel`, `aria-selected`, le focus roving et la navigation clavier par flèches, Home et End. Les autres modales de l’application restent inchangées.
- Version applicative/installeur : **0.3.6**. Suite complète : **163 tests passants**. `node --check static/desktop.js` et `git diff --check` sont propres.
- Build Windows local validé : `dist/SEL-Facturation.exe` et `dist/installer/SEL-Facturation-Setup-0.3.6.exe` générés. Les avertissements PyInstaller concernent uniquement des imports optionnels WebView2 Android et pycparser ; le build est terminé avec succès.

## Addendum — brouillons de formulaires (0.3.7)

- Les formulaires métier conservent maintenant leur saisie dans la session Flask après une validation refusée et la restaurent après actualisation : nouveau décompte, création/modification d’abonné, paramètres SEL, paiement et annulation.
- Une sauvegarde temporaire `sessionStorage` couvre aussi une actualisation avant l’envoi. Elle est limitée au formulaire concerné, exclut les champs de mot de passe et ne remplace jamais la session serveur après une erreur.
- Les contrôles de recherche, de déconnexion et d’authentification ne sont pas transformés en brouillons. Les formulaires validés nettoient leur copie temporaire pour éviter de réinjecter une ancienne saisie lors d’une prochaine visite.
- Version applicative/installeur : **0.3.7**. Suite complète : **170 tests passants**. `node --check static/desktop.js` et `git diff --check` sont propres. Build Windows local validé : `dist/SEL-Facturation.exe` et `dist/installer/SEL-Facturation-Setup-0.3.7.exe` générés. Les avertissements PyInstaller restent limités aux imports optionnels WebView2 Android et pycparser.
- La validation graphique et l’installation sur le poste matériel SEL restent à effectuer ; statut : **pilote non validé sur poste cible**.

## Addendum — attribution légale et durcissement de livraison (0.3.8)

- SEL reste l’identité métier principale. Les logos MonarQue Group et LTEX sont limités à l’écran « À propos et mentions légales » ; le shell affiche uniquement une attribution discrète `Designed and secured by LTEX · MonarQue Group`. Aucun logo développeur n’est ajouté aux PDF client.
- Les sauvegardes quotidiennes sont maintenant séparées du nettoyage des snapshots pré-mise à jour et des exports manuels.
- Le pré-vérificateur reconnaît les copies portables déjà lancées au moyen d’un marqueur local sans données sensibles ; les copies arbitraires jamais exécutées ne sont pas prétendues détectables.
- La validation Authenticode de l’updater refuse désormais les sujets de certificat qui ne correspondent pas exactement à l’identité configurée.
- Le tag de publication GitHub est dérivé de `APP_VERSION` et ne réutilise plus un tag historique codé en dur.
- Version candidate : **0.3.8**. La signature Authenticode et la recette sur le poste SEL restent à effectuer ; statut : **pilote non validé sur poste cible**.

### Preuves de build 0.3.8 — 17/10/2026

- Suite complète locale : **182 tests passants**.
- Vérifications statiques : `compileall`, `node --check static/desktop.js` et `git diff --check` passants.
- Build Windows local : `BUILD_EXIT=0` avec PyInstaller et WiX 4.
- `SEL-Facturation.exe` — SHA-256 `E6371D96223C7A9A8CE0503AF286AE4D33AEB120BDD1B46E9413399313F3E152`.
- `SEL-Facturation.msi` — SHA-256 `ED151366D00379F69C1ED6DC5CD9AA0E8A5E050F3762E9A096A3793550F47F7B`.
- `SEL-Facturation-Setup-0.3.8.exe` — SHA-256 `C8451689E1A4FDE0A5DA0BDF8761F48D0B4B2EF766D3BAA75F7D13EB2968F15A`.
- Signature Authenticode locale des trois artefacts : **NotSigned** ; aucun artefact ne doit être présenté comme version de production avant signature.
- Avertissements PyInstaller observés : imports optionnels Android WebView2 et tables optionnelles pycparser uniquement.
- Validation d’installation, mise à niveau, restauration, WebView2 et impression sur le poste SEL : **non effectuée** ; statut : **pilote non validé sur poste cible**.

### Addendum de revue indépendante — corrections finales

- Le test d’icône ne dépend plus du fichier `.spec` généré et ignoré par Git ; un checkout propre peut donc exécuter la suite sans artefact local.
- Le pré-vérificateur traite une copie portable ou une inscription MSI de même version comme une installation de maintenance via le bootstrapper courant lorsqu’aucun bundle Burn enregistré n’est disponible ; le mode `/repair` reste réservé au bundle Burn mis en cache.
- Les snapshots `sel-preupdate-*` disposent d’une rétention dédiée bornée à 30 copies. Ils restent exclus du nettoyage quotidien, tout en évitant une croissance illimitée lors des démarrages répétés.
- La détection WebView2 utilise les deux emplacements officiels Evergreen : HKLM machine et HKCU utilisateur, avec l’identifiant documenté `{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`.
- Le workflow provisionne l’extension WiX Util et signe dans l’ordre les exécutables internes, le MSI avant Burn, le bundle Burn avant son encapsulation par le pré-vérificateur, puis le lanceur final. Cela évite de signer uniquement l’enveloppe après inclusion de charges non signées.
- Nouvelle validation locale après ces corrections : **182 tests passants**. La reconstruction Windows et la validation cryptographique avec un certificat SEL restent nécessaires ; sans certificat, les artefacts locaux restent **NotSigned** et le statut demeure **pilote non validé sur poste cible**.

### Incident d’installation 0.3.8 et correctif 0.3.9

- Le journal Burn du poste utilisateur a confirmé `0x80070005` sur `C:\ProgramData\Package Cache`, lors de la préparation de `WebView2EvergreenStandalone`. Le bundle SEL était par utilisateur, mais le paquet WebView2 était déclaré `PerMachine="yes"`; le compte non élevé ne pouvait donc pas sécuriser le cache machine.
- WebView2 est maintenant déclaré par utilisateur (`PerMachine="no"`) afin de rester cohérent avec le MSI SEL `Scope="perUser"`. La détection conserve les emplacements runtime HKLM et HKCU, et l’installation peut fonctionner sans privilèges administrateur lorsque Windows l’autorise.
- La correction est versionnée **0.3.9** afin que Burn ne réutilise pas le bundle 0.3.8 déjà enregistré sur la machine.
- Une nouvelle validation sur le poste cible reste indispensable : le comportement dépend aussi de la politique UAC, de l’antivirus et des droits Windows locaux.

### Preuves de build 0.3.9 — correctif accès cache

- Suite complète locale : **182 tests passants**.
- `build-windows.ps1` : `BUILD_EXIT=0`; compilation WiX du bundle : code 0.
- `SEL-Facturation.exe` — SHA-256 `BB4FA84960532E052F299A3AFBD21E81D6BC174DA7075C2E85108C8772F86D9B`.
- `SEL-Facturation.msi` — SHA-256 `AEA6C5E9BD7C63E46B5DD3CFB8EAD9D4E593B60FB90DEE89F185049C6C5F452E`.
- `SEL-Facturation-Setup-0.3.9.exe` — SHA-256 `70212D3823073E2BD1447BDAE068B1AE006981F68AC3CBF872CA9AD8F910C06A`.
- Les artefacts locaux sont `NotSigned` en l’absence du certificat Authenticode SEL. Le journal du poste cible devra confirmer l’installation réelle ; statut : **pilote non validé sur poste cible**.
