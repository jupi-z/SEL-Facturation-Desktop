# Plan minimal d'audit — livraison SEL Facturation Desktop

## P0 — parcours bloquants à reproduire

- Vérifier les actions PDF dans la fenêtre native Windows avec une association d'impression absente. Le comportement attendu est d'ouvrir le PDF et d'indiquer clairement l'impression manuelle via Ctrl+P.
- Vérifier la cohérence du numéro de version exposé par l'application avec la préversion `v0.2.0-rc1` et l'installateur `0.2.0`.
- Exécuter la suite backend/PDF/HTTP/desktop, puis la compilation PyInstaller et, si disponible, Inno Setup.

## P1 — conservation et risques à documenter

- Vérifier qu'une sauvegarde automatique journalière inclut les données écrites après le premier démarrage de la journée.
- Vérifier les calculs Decimal, les assiettes de taxe/TVA, les doublons, paiements partiels, annulations et snapshots immuables.
- Vérifier le loopback, les protections HTTP/CSRF/CSP, les exports portables v1 et l'absence de données de session.
- Vérifier les limites non testables dans cet environnement : WebView2, imprimante SEL, installateur et restauration physique sur poste cible.

Les corrections restent additives, limitées au dépôt existant, sans changement de pile ni modification du schéma portable v1.
