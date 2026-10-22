# SEL Facturation Desktop — V0.3.26 locale native Windows

Application de bureau locale pour la **Société d'Énergie du Lualaba (SEL)** : gestion des abonnés/compteurs, relevés et périodes, calcul par tranches de kWh, taxe locale et TVA configurables, décomptes A4 PDF, historique, enregistrement de paiements, sauvegarde et export de données.

**Statut : version pilote locale, à tester et valider sur un PC Windows de SEL avant toute utilisation commerciale.** Les PDF clients sont intitulés « décomptes d'électricité », sans encadré de statut fiscal anxiogène et sans fausse référence, signature ou QR DGI. L'identité officielle SEL et les règles tarifaires confirmées sont préchargées ; la fiscalisation officielle reste une procédure distincte. Ne jamais présenter ces PDF comme des factures normalisées ou homologuées sans authentification par le dispositif autorisé.

### Règles de facturation actuellement implémentées

- Facturation mensuelle selon la consommation en kWh, saisie directement tant que les compteurs ne sont pas disponibles.
- Abonné résidentiel : 0,095 USD/kWh hors TVA.
- Abonné commercial : 0,12 USD/kWh hors TVA.
- TVA : 16 %. Aucune taxe locale n'est appliquée aux nouvelles factures.
- Devise principale : USD. Une variante CDF peut être produite avec un taux configurable, initialement 1 USD = 2 360 CDF.
- Les compteurs et relevés restent facultatifs et pourront être activés ultérieurement.
- Les anciens barèmes progressifs et anciennes taxes restent lisibles dans les factures historiques mais ne contrôlent pas le nouveau parcours.

### Configuration administrable

Les paramètres métier se gèrent depuis **Paramètres de facturation**, sans modifier le code :

- types d’abonnés actifs et tarif initial par kWh ;
- tarifs datés par type, conservés avec leur période d’application ;
- taux USD/CDF datés, utilisés par la variante CDF ;
- préfixe et largeur des numéros de décompte ;
- méthodes de paiement actives et exigences de référence ;
- nombre de sauvegardes automatiques conservées.

Les tarifs, devises et politiques sont stockés dans des catalogues SQLite structurés. Les anciennes clés `settings`, les anciens barèmes et l’export portable v1 restent conservés pour compatibilité. Une facture émise contient toujours les valeurs réellement appliquées au moment de son émission et n’est jamais recalculée après une modification de configuration.

## Démarrage développeur

Python 3.11 ou 3.12 sur Windows :

```powershell
py -3.12 -m venv .venv
.venv\Scripts\python -m pip install -r requirements.txt
.venv\Scripts\python desktop.py
```

L'interface s'ouvre dans sa propre **fenêtre Windows native** (pywebview + WebView2) : aucun navigateur externe n'est lancé pour utiliser SEL Facturation. Un mot de passe administrateur d'au moins 10 caractères est demandé à la première utilisation. Le serveur écoute **uniquement sur `127.0.0.1`**. Il ne doit jamais être exposé sur Internet ou un réseau public : ce n'est pas une application multi-postes.

## Version Windows pour les agents (sans Python)

Créer l'exécutable sur un PC **Windows 64 bits**, ou via le workflow GitHub Actions `Build Windows Desktop` :

```powershell
py -3.12 -m venv .venv
.venv\Scripts\python -m pip install -r requirements-build.txt
.venv\Scripts\pyinstaller --clean --noconfirm --onefile --noconsole --name "SEL-Facturation" --icon "packaging\sel-facturation.ico" --add-data "templates;templates" --add-data "static;static" --hidden-import clr --collect-all webview desktop.py
```

`dist\SEL-Facturation.exe` lance la fenêtre Windows dédiée avec l'icône du logo SEL ; aucun navigateur externe n'est nécessaire pour les écrans de gestion. Pour la distribution, le workflow Windows compile un MSI par utilisateur et le bootstrapper WiX qui installe silencieusement le WebView2 Evergreen Standalone puis le MSI. Le bootstrapper WiX est l'unique paquet de livraison supporté. `packaging\SEL-Facturation.iss` est conservé uniquement pour compatibilité historique et ne doit plus être utilisé pour une nouvelle installation ou une mise à jour. Ne jamais distribuer une version sans recette et sauvegarde. **Un binaire non compilé et non vérifié ne doit pas être annoncé comme disponible.**

## Où sont les données ?

Sous Windows : `%LOCALAPPDATA%\SEL Facturation\sel.sqlite3` (base SQLite), `session.key` et le répertoire `sauvegardes`. Elles restent séparées du dossier d'installation et de l'exécutable, donc ne sont pas écrasées à la mise à jour.

- Au démarrage, une sauvegarde SQLite datée est conservée (rétention : 14 fichiers journaliers).
- Le menu **Sauvegarde SQLite** produit une copie cohérente téléchargeable.
- Sauvegarder régulièrement la copie hors du PC (disque externe chiffré, stockage contrôlé).
- Le programme **n'effectue pas de synchronisation entre ordinateurs** ; ne pas ouvrir la même base via un partage réseau.
- Pour restaurer, fermer l'application puis confier le remplacement de la base par une personne compétente, avec sauvegarde préalable et vérification d'intégrité.

## Parcours agent

1. Créer le mot de passe au premier lancement.
2. Dans **Paramètres**, vérifier les références SEL préchargées et **confirmer** les tarifs, le taux de change et l'assiette de TVA avant la première émission.
3. Dans **Abonnés**, enregistrer nom, adresse, contrat, numéro de compteur, coefficient et NIF le cas échéant.
4. Dans **Nouvelle facture**, choisir l'abonné, mois, deux index et date d'échéance.
5. Le serveur calcule les tranches, les taxes configurées, le total et attribue un numéro unique.
6. Utiliser les boutons **Voir le PDF**, **Enregistrer PDF** ou **Imprimer** : ouverture et impression avec le lecteur PDF associé à Windows, enregistrement par boîte de dialogue native ; le navigateur intégré reste réservé à l'interface du programme.
7. Dans **Factures**, retrouver/re-imprimer, enregistrer un paiement ou annuler un document **non réglé** avec motif.
8. Exporter périodiquement une sauvegarde et le JSON portable.

### Fonctionnement et limites assumées

- Un seul document **actif** par client et par période, contrôlé aussi par index unique SQLite.
- Le compteur n'est pas historisé indépendamment : son numéro et le coefficient sont copiés dans chaque facture. Un changement de compteur est à traiter manuellement, avec validation avant émission.
- Les anciens index sont saisis manuellement : aucun report implicite. Éviter les erreurs en vérifiant le relevé précédent.
- Les taux de TVA et taxe locale et le barème sont configurables, mais leur légalité n'est **pas validée automatiquement**.
- Le montant de TVA de la facture source fournie par SEL ne correspond pas aux assiettes visibles : ne pas tenter de reproduire cet écart.
- Les paiements sont des **constats saisis**, pas des transactions bancaires/Mobile Money exécutées.
- Il n'existe qu'un compte administrateur par installation dans cette première version ; pas de journal d'audit nominatif multi-agents.
- Pas d'envoi SMS, pas de connexion DGI, pas de facturation de masse, pas de multi-postes, pas de comptabilité SYSCOHADA.
- Pour la **fenêtre native Windows**, le bootstrapper de livraison installe Microsoft Edge WebView2 Runtime si nécessaire. Le programme ne fonctionne pas comme application Windows si ce composant manque ; si la stratégie informatique bloque l'installation silencieuse, un administrateur doit l'autoriser.
- La visualisation du PDF et sa commande d'impression utilisent le lecteur PDF associé à Windows. La commande « Imprimer » doit être testée avec l'imprimante de SEL ; l'interface de ce lecteur est externe à l'application principale.
- Sous Windows le programme empêche une seconde instance simultanée. Les PDF prévisualisés sont temporairement sauvegardés sous `%LOCALAPPDATA%\SEL Facturation\documents-temporaires`.

- Les modifications de la fiche client ou du barème ne recalculent jamais les snapshots des documents émis.

## Portabilité future vers Monarque Connect

Télécharger **Export JSON complet** depuis l'application. Il s'agit du format `sel-facturation-portable`, `schema_version: 1` :

| Fichier / objet | Clé métier stable | Destination possible dans Monarque Connect |
|---|---|---|
| `data.clients[]` | `code` (`SEL-C-00001`) | `BusinessParty` + référence contrat/compteur spécialisée |
| `data.invoices[]` | `number` (`SEL-YYYY-NNNNNN`) | `Invoice` ou import historique métier énergie |
| `data.payments[]` | `invoice_number` + `id` | `PaymentRecord` / historique de paiement |
| `data.company_settings` | noms des clés | Configuration d'organisation et grille tarifaire énergie |

Les `client_snapshot`, `company_snapshot`, `calc_snapshot` de chaque document sont conservés à l'identique. L'import doit être **versionné, idempotent, et validé avant rapprochement** ; les identifiants SQLite entiers sont des références locales, pas les futurs UUID Monarque. On conserve aussi des exports CSV par catégorie. Aucun mot de passe ni clé de session n'est inclus dans les exports.

L'adaptateur Monarque Connect n'est **pas développé** dans ce dépôt. Sa conception dépendra de l'API et du modèle Energy futurs ; on évite ici toute fausse compatibilité directe avec `Invoice` existant, lequel exige aujourd'hui une commande confirmée.

## Vérifications

```powershell
.venv\Scripts\python -m pip install pytest
.venv\Scripts\python -m pytest -q
```

Pour livrer aux agents, exécuter le scénario de recette sur un **Windows cible** : premier démarrage, fermeture/redémarrage, création de deux clients, facture d'essai, sauvegarde, réimpression, tentative de doublon, paiement partiel, connexion/déconnexion, test d'impression A4 et restauration sur une copie. Vérifier les montants et la fiscalité avec SEL avant de traiter les vrais clients.

## Publication dans le dépôt privé fourni

Après extraction de l'archive source sur votre PC de développement Windows, double-cliquez sur PowerShell ou exécutez dans le répertoire du projet :

```powershell
powershell -ExecutionPolicy Bypass -File .\publish-to-github.ps1
```

Le script clone `jupi-z/SEL-Facturation-Desktop`, copie les sources, crée un commit et pousse sur `main`. Il exige **Git** et une session GitHub autorisée. Aucun mot de passe ni jeton n'est stocké dans le script. Le workflow `Build Windows Desktop` peut alors produire les exécutables Windows dans les artefacts GitHub Actions après réussite des tests.

Si les sources sont déjà sur la machine Windows, `powershell -ExecutionPolicy Bypass -File .\build-windows.ps1` exécute la suite de tests puis compile l’EXE, le MSI WiX et le bootstrapper WiX Burn.

Le build officiel utilise désormais exclusivement WiX Burn pour le bootstrapper et WiX MSI ; Inno Setup est conservé uniquement comme compatibilité historique. Les fichiers distribués doivent être signés et validés sur le poste Windows de SEL avant utilisation en production.

## Développeur et attribution

SEL Facturation est développé par le pôle informatique LTEX de MonarQue Group S.A.S. Les mentions légales complètes et les informations publiques du développeur sont accessibles dans l’écran « À propos et mentions légales » de l’application. Le logiciel reste une application locale de la Société d’Énergie du Lualaba ; aucun logo développeur n’est imprimé sur les documents clients.
