# Candidat Windows SEL V2

Le paquet V2 est un candidat de test non signé Authenticode. Il contient :

- `SEL-Facturation-V2.exe`, shell natif pywebview et agent `sel-local.exe` embarqué ;
- un MSI par utilisateur ;
- un bootstrapper WebView2 qui installe le runtime Microsoft uniquement si le poste ne le possède pas ;
- `SHA256SUMS.txt` pour vérifier les artefacts avant installation.

La base SQLite et les identités sont créées dans `%LOCALAPPDATA%\SEL Facturation V2`. Aucun PDF ni fichier de facture n’est envoyé ou stocké sur le serveur central par ce paquet.

## Build Windows

Depuis la racine du dépôt, avec Rust, Python 3.12, WiX 4 et les dépendances de `requirements-build.txt` :

```powershell
python -m pip install -r requirements-build.txt
pwsh -File v2/packaging/build-v2.ps1 -Version 0.3.30
```

Le téléchargement WebView2 provient du lien officiel Microsoft utilisé uniquement au moment du build. Le bootstrapper conserve le runtime hors de l’application et ne contient aucun secret SEL.

La validation réelle sur le poste SEL, l’impression, la configuration centralisée et la signature Authenticode restent des contrôles séparés.
