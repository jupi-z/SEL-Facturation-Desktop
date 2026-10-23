# Matrice de tests, recette Windows et critères GO / NO-GO

Codex doit fournir la **preuve**, non simplement la déclaration, pour chaque série. Les tests Windows peuvent utiliser des artefacts et clés de test; la première release réelle nécessite une **clé de production distincte**. Toute vérification « Windows cible SEL » non effectuée est marquée NON TESTÉE, jamais PASS.

## 1. Cryptographie et provenance (P0)
| ID | Situation | Résultat attendu |
|---|---|---|
| CR-01 | Manifeste bytes signés Ed25519 avec clé publique embarquée | Accepté après vérification, même si Authenticode est absent |
| CR-02 | Une seule lettre du manifeste change | Refus avant décision et avant téléchargement |
| CR-03 | Signature absente/mal formée ou key-id inconnu | Refus avec diagnostic, aucune exécution |
| CR-04 | Signature correcte sur autre manifeste | Refus |
| CR-05 | Manifeste correct mais package modifié de 1 octet | Refus hash/size, suppression quarantinée |
| CR-06 | Champ `url` modifié hors signature | Refus |
| CR-07 | JSON à clés dupliquées, NaN, UTF-8 invalide, > limite | Refus |
| CR-08 | HTTPS → HTTP redirect, URL file://, localhost/loopback | Refus |
| CR-09 | Version égale/inférieure, séquence régressive, expiré | Refus/état current propre, pas installation |
| CR-10 | Signature correcte dans canal preview mais client stable | Pas d'offre, sauf opt-in explicite |
| CR-11 | Deux mises à jour tentées simultanément | Une seule transaction |
| CR-12 | Clé privée de prod absente | Build test OK, **publication réelle impossible** |
| CR-13 | Paquet téléchargé modifié après validation | Re-vérification juste avant lancement, refus |
| CR-14 | Nouveau client ne possède pas de clé publique valide | **Fail closed**, jamais fallback vers « HTTPS + SHA seulement » |
| CR-15 | Manifeste/sig de commits CDN différents | Réessai borné; sinon refus |
| CR-16 | Nouveau protocole avec `windows_code_signed:false` et manifeste valide | Chemin autorisé sans contrôle Authenticode obligatoire |
| CR-17 | Fausse signature test en environnement production | Refus |

## 2. Application et ergonomie (P0/P1)
- UX-01 : SEL s'ouvre **immédiatement hors ligne**; le check réseau est asynchrone, timeout raisonnable, UI sans freeze.
- UX-02 : aucune update = pas de notification intempestive; nouvelle version = « Mettre à jour / Plus tard » dans une zone discrète.
- UX-03 : clic « Mettre à jour » puis affichage **réel** des octets téléchargés et états successifs; aucun pourcentage simulé.
- UX-04 : annuler le téléchargement, relancer et reprendre proprement sans corrompre staging.
- UX-05 : fermer/relancer en plein téléchargement, pendant sauvegarde, à l'étape d'installation; reprise sûre et message adéquat.
- UX-06 : onglet updates utilisable par clavier/WebView2; pas de clic répété conduisant à plusieurs installations.
- UX-07 : erreur authentification manifeste / téléchargement = aucune modification DB ni arrêt des opérations métier.
- UX-08 : données privées, clients et paiements absents des journaux d'update; notes affichées échappées sans HTML injecté.
- UX-09 : transition 0.3.24 → première version SafeUpdate : **passage manuel identifié**, non dissimulé.

## 3. SQLite et récupération (P0)
- DB-01 : fichier existant sauvegardé par SQLite backup API, `PRAGMA integrity_check=ok`, chemin et taille vérifiés; migration uniquement après snapshot.
- DB-02 : 2 clients, facture(s) émises, paiement partiel, numéros, JSON historique et statuts inchangés après upgrade.
- DB-03 : données et exports `sel-facturation-portable` v1 avant/après équivalents pour l'historique antérieur; nouvelles migrations additives.
- DB-04 : échec backup / disque plein / base verrouillée => installation bloquée, app fonctionnelle ou diagnostic exploitable.
- DB-05 : mise à jour interrompue ou MSI en erreur : données intactes, plan de récupération explicite, paquet précédent vérifiable.
- DB-06 : deux processus SEL / mutex / DB WAL, fermeture propre avant remplacement binaire; aucune perte des écritures en cours.
- DB-07 : pas de restauration silencieuse d'ancienne DB en présence d'une migration évolutive incompatible.

## 4. Matrice Windows (P0)
| Scénario | Poste | Preuve |
|---|---|---|
| W-01 | Windows 10 x64 compte **standard** propre, WebView2 installé | Install bootstrapper non signé selon politique de poste, upgrade per-user, relance |
| W-02 | Windows 11 x64 compte **standard**, WebView2 absent | Runtime per-user quand autorisé; signaler si policy bloque |
| W-03 | Ancien MSI WiX per-user | Upgrade correct, une seule entrée programme, DB préservée |
| W-04 | Ancien Inno Setup | Précontrôle/migration ou blocage sûr sans double installation |
| W-05 | EXE portable isolé | Détection et décision « installer / migration » documentées |
| W-06 | Installation de même version | Aucun reinstall inutile, maintenance explicitement limitée |
| W-07 | Downgrade | Refus |
| W-08 | SmartScreen/Smart App Control bloque unsigned | Erreur honnête + recours légitime, **aucun bypass** |
| W-09 | Compte admin local démarrant l'app | **Pas de requireAdministrator**; demandes UAC possibles seulement si paquet nécessite réellement |
| W-10 | Coupure réseau pendant download | Facturation conservée, staging non exécuté |
| W-11 | Arrêt brutal durant installation | Diagnostic / recovery sans perte SQLite |
| W-12 | Plusieurs mises à jour successives > 2 | Bonne version, pas de double entrée ni données perdues |

## 5. Tests CI et preuves attendues
- `python -m pytest -q` complète, tests ciblés updater et sécurité ; zéro test « attendu cassé » ignoré silencieusement.
- Tests cryptographiques **réels** avec `cryptography` (clés de test), fixtures « unsigned Windows + Ed25519 valide » et « unsigned + aucune preuve = rejet ».
- `python -m compileall`, `git diff --check`, lint adéquat, PyInstaller exe, WiX MSI, Burn/bootstrapper, vérification des hashes.
- Matrice Win 10 / Win 11 : au moins une installation E2E et mise à jour de machine virtuelle **ou marquer non effectuée**; un build GitHub runner `windows-latest` ne remplace pas une recette sur PC cible.
- Générer un manifeste+signature de démonstration dans environnement de test et attester que le binaire installé le vérifie, y compris après redémarrage.
- Audit fonctionnel : émettre facture, enregistrer paiement, exporter et imprimer PDF avant/après la mise à jour; préserver l'éventuelle évolution eau+électricité.
- Tests de régression des paramètres et packaging (installeur Legacy, version, chemin, édition utilisateur).
- Créer `Docs/SEL_SAFEUPDATE_V1/IMPLEMENTATION_REPORT.md` détaillant test/plateforme/résultat/preuve/non-test, SHA commit, version package, chemins staging/recovery, screenshot UX.

## Définition du GO
**GO technique de revue PR :** tests automatisés verts, package compilé, sécurité Ed25519 prouvée, pas de fuite secrets, aucune modification DB métier, PR non fusionnée.
**GO pilote PC SEL :** installation d'une version bootstrap contrôlée + deux updates successives, sans privilèges administrateur lorsque Windows le permet, SQLite conservée, compte standard, impression, accès hors connexion, gestion d'échec reproductible et validation SEL.
**NO-GO impératif :** clé privée exposée; téléchargement quelconque exécuté sans signature; perte de données; fail-open; contournement UAC/SmartScreen; rollback non sûr; réseau requis pour facturer; prétention « zéro intervention Windows » sans test sur poste réel.
