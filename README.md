# SEL Facturation Desktop

Application Windows locale de facturation de la Société d'Énergie du Lualaba (SEL), indépendante de Monarque Connect.

## État du dépôt

Le dépôt a été initialisé. Le projet complet a été créé sous forme d'archive de sources dans la conversation ChatGPT et doit encore être publié ici. Le contenu de cette archive comprend : application Flask, persistance SQLite, moteur de facturation, PDF A4, interface, tests, exports CSV/JSON et scripts Windows.

Après avoir téléchargé et extrait `SEL_Facturation_Desktop_Source.zip`, exécuter dans le dossier du projet sur un PC avec Git et authentification GitHub :

```powershell
powershell -ExecutionPolicy Bypass -File .\publish-to-github.ps1
```

Le script pousse les sources vers ce dépôt. Le workflow `Build Windows Desktop` contenu dans les sources construira ensuite un exécutable portable et tentera de construire un installateur Windows.

**Important :** les documents PDF générés sont des décomptes non authentifiés par la DGI. Les tarifs et la fiscalité doivent être validés par SEL avant utilisation réelle. La connexion à la DGI n'est pas implémentée.

Le système exporte des données portables (`sel-facturation-portable`, version 1) avec les identifiants clients et numéros de factures stables, en vue d'une migration future vers Monarque Connect.
