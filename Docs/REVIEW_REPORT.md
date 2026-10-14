# Rapport d'audit — SEL Facturation Desktop

Date : 14 octobre 2026
Branche : `monarque/audit-livraison-sel`
Issue : [#3](https://github.com/jupi-z/SEL-Facturation-Desktop/issues/3)

## Périmètre et méthode

Audit de `AGENTS.md`, `Docs/CODEX_HANDOFF.md`, `README.md`, des modules Python principaux, des templates/static, de la suite `tests/`, du workflow Windows et de l'installateur Inno Setup. Les défauts corrigés ont d'abord été reproduits par tests de régression en échec, puis vérifiés après correction.

## Anomalies confirmées et corrections

### P0 — impression Windows sans repli explicite

`desktop.py` appelait directement `os.startfile(path, "print")`. Si l'association Windows ne fournit pas le verbe `print`, l'action échouait sans ouvrir le document. Le pont ouvre maintenant le PDF dans le lecteur associé et renvoie un état `fallback=open` avec l'instruction Ctrl+P ; `static/desktop.js` l'affiche comme une aide, pas comme une erreur trompeuse.

### P1 — sauvegarde journalière obsolète

`db.automatic_backup()` conservait le fichier du jour sans le rafraîchir. Une base modifiée après le premier démarrage pouvait donc ne pas être incluse dans la sauvegarde quotidienne suivante. Le snapshot du jour est désormais régénéré à chaque démarrage/appel de la routine, avec le même emplacement et sans suppression de données métier.

La régénération est écrite dans un fichier temporaire, vérifiée par `PRAGMA integrity_check`, puis remplacée atomiquement ; l'ancien snapshot reste disponible si la copie échoue.

### P1 — version de santé incohérente

`/health` annonçait `1.0.0` alors que la préversion et l'installateur sont `0.2.0`. La version applicative est maintenant centralisée dans `app.py` à `0.2.0` pour ce cycle.

## Vérifications réalisées

- Suite complète : `25 passed` avec Python intégré 3.12 et les dépendances du dépôt.
- `git diff --check` : aucune erreur de whitespace.
- Compilation PyInstaller locale : `Build complete!`, `dist/SEL-Facturation.exe` créé ; l'analyse a émis des avertissements de collecte facultative WebView2/pycparser à confirmer dans la CI Windows.
- Tests de régression dédiés : sauvegarde rafraîchie, repli impression, message JavaScript et version `/health`.
- Vérification statique : loopback `127.0.0.1`, CSRF, cookies HttpOnly/SameSite, CSP, requêtes paramétrées, exports sans clé de session/mot de passe, snapshots protégés par trigger SQLite, calculs métier en `Decimal`.

## Limites et non-validations

- Le lanceur `py -3.12` n'est pas installé dans l'environnement local. La suite a été exécutée avec le runtime Python 3.12 intégré à Codex ; le script `build-windows.ps1` n'a donc pas été exécuté tel quel.
- Inno Setup n'est pas installé dans l'environnement local ; l'installateur n'a pas été compilé ici.
- Aucune validation physique n'a été possible avec WebView2, un poste SEL, une imprimante, l'antivirus ou l'installateur Inno Setup. Le statut reste **pilote non validé sur poste cible**.
- La CI Windows et la compilation/release doivent encore être confirmées par le workflow GitHub sur la PR. Aucun binaire ni lien de release n'est annoncé ici.
- Les tarifs, taxes, bases et identifiants d'entreprise restent à confirmer par SEL ; aucun statut DGI, QR ou identifiant fiscal fictif n'est ajouté.

## Avis de livraison

**NO-GO pour remise commerciale immédiate.** Le socle corrigé est prêt pour revue et CI Windows, mais la livraison chez SEL ne peut passer en GO qu'après CI verte, compilation/installateur vérifiés et recette sur le poste Windows/imprimante cible, avec restauration d'une copie de sauvegarde testée.

PR et commit seront ajoutés à ce rapport après publication de la branche.
