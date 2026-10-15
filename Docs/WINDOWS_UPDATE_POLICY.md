# Politique de mise à jour Windows

Le paquet de livraison officiel est `SEL-Facturation-Setup-<version>.exe`, le bootstrapper WiX. Il embarque le MSI et l'installateur autonome WebView2 afin qu'une machine Windows 10/11 puisse être préparée en une seule installation.

## Garanties du parcours officiel

- Le MSI conserve le même `UpgradeCode` et le bootstrapper conserve le même `UpgradeCode` à chaque version.
- Le MSI installe le programme dans `%LOCALAPPDATA%\Programs\SEL Facturation`.
- La base, les sauvegardes, les documents temporaires et la clé de session restent dans `%LOCALAPPDATA%\SEL Facturation`, en dehors du dossier remplacé par l'upgrade.
- `MajorUpgrade` refuse les rétrogradations. Une mise à jour doit toujours augmenter `APP_VERSION` dans `version.py` ; une nouvelle MSI avec un autre identifiant produit mais la même version n'est pas considérée comme une mise à jour fiable.
- La CI installe une MSI précédente puis le bootstrapper officiel de la nouvelle version, vérifie l'intégrité SQLite et le contenu exact d'un client, d'une facture, d'un paiement et de leurs snapshots, puis vérifie le témoin de données et l'unicité de l'entrée d'installation.
- Le paquet Inno Setup est conservé pour compatibilité développeur et n'est pas publié dans la release officielle. Le bootstrapper WiX bloque explicitement une ancienne installation Inno détectée ; il faut alors désinstaller l'ancien paquet depuis Windows, vérifier les sauvegardes, puis lancer le paquet WiX.

## Procédure de release

1. Incrémenter `APP_VERSION` dans `version.py` (par exemple `0.3.0` vers `0.3.1`).
2. Laisser le workflow compiler l'EXE, la MSI et le bootstrapper.
3. Exécuter la recette Windows et la sauvegarde/restauration sur le poste cible SEL.
4. Déclencher manuellement le workflow avec un tag dont la version numérique correspond à `APP_VERSION`, par exemple `v0.3.1-rc1`.
5. Installer la release sur une machine qui possède déjà la version précédente et vérifier l'ouverture, les données, les PDF et les raccourcis.

## Limite à déclarer

Cette garantie concerne le parcours WiX officiel. Une archive portable ou un paquet tiers inconnu ne peut pas être désinstallé de manière fiable par le MSI ; il doit être identifié et retiré manuellement après sauvegarde avant déploiement. La validation sur le matériel et le profil Windows de SEL reste obligatoire.
