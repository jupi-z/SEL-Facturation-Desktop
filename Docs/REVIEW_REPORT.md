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
- La validation graphique et l’installation sur le poste matériel SEL restent à effectuer ; statut : **pilote non validé sur poste cible**.
