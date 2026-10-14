# Package MSI

Le MSI installe uniquement l'application dans le profil utilisateur. Il ne
déclare pas `%LOCALAPPDATA%\SEL Facturation` comme composant MSI : la base
SQLite, les PDF et les sauvegardes restent donc hors du périmètre de
désinstallation et de mise à niveau.

Compilation WiX 4 depuis la racine du projet :

```powershell
wix build packaging/wix/Product.wxs `
  -d ProductVersion=0.3.0 `
  -d ExePath=dist/SEL-Facturation.exe `
  -o dist/installer/SEL-Facturation.msi
```

Le bootstrapper est construit par le script Windows et ajoute le package
WebView2 Evergreen Standalone avant le MSI.
