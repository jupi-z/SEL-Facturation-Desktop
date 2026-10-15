# Plan d’implémentation — Mise à jour automatique Windows

## Résultat attendu

Ajouter à SEL Facturation Desktop une vérification de mise à jour non bloquante, une page de contrôle manuel et un lancement sécurisé du bootstrapper WiX, sans remplacer l’EXE en cours ni modifier les données métier.

## Contraintes

- Python/Flask/SQLite et le format portable v1 restent inchangés.
- Windows 10/11 x64 est la cible officielle.
- Aucun téléchargement HTTP, downgrade, exécution arbitraire ou mise à jour silencieuse.
- Le réseau indisponible ne doit pas empêcher l’application de démarrer.
- Une sauvegarde SQLite vérifiée est obligatoire avant lancement de l’installeur.
- La signature Authenticode est signalée comme exigence de production ; le mode non signé reste explicitement pilote.

## Fichiers concernés

- Créer `updater.py` : modèle du manifeste, validation HTTPS/JSON/version/hash, téléchargement temporaire et préparation de sauvegarde.
- Modifier `app.py` : routes protégées `/updates` et `/updates/check`, état de mise à jour non sensible, lancement délégué à `desktop.py`.
- Modifier `desktop.py` : fonction native de lancement du bootstrapper validé puis fermeture propre de l’application.
- Modifier `templates/base.html` : entrée discrète vers la page des mises à jour.
- Créer `templates/updates.html` : statut, version disponible, erreurs récupérables et confirmation explicite.
- Modifier `static/desktop.js` uniquement si l’interface nécessite l’ouverture/fermeture native avant lancement.
- Modifier `tests/` : tests unitaires du manifeste, tests HTTP des routes et test du pont natif sans exécution réelle.
- Modifier `README.md` et `Docs/WINDOWS_UPDATE_POLICY.md` : procédure, limites de signature et statut pilote.
- Modifier `version.py` seulement lors de la release finale ; cette feature ne changera pas la version par elle-même.

## Interfaces arrêtées

`updater.py` exposera :

- `UpdateManifest.from_payload(payload: bytes, source_url: str) -> UpdateManifest`
- `UpdateManifest.is_newer_than(current: str) -> bool`
- `check_for_update(manifest_url: str, current_version: str, timeout: float = 3.0) -> UpdateCheckResult`
- `download_update(manifest: UpdateManifest, destination_dir: Path, timeout: float = 30.0) -> Path`
- `verify_sha256(path: Path, expected: str) -> None`

Le manifeste minimal sera :

```json
{
  "schema": 1,
  "version": "0.3.2",
  "url": "https://.../SEL-Facturation-Setup-0.3.2.exe",
  "sha256": "64 caractères hexadécimaux",
  "size": 12345678,
  "published_at": "2026-10-15T12:00:00Z",
  "release_notes": "..."
}
```

L’URL doit être HTTPS, le fichier doit finir par `.exe`, la taille doit être positive et bornée, le hash doit être hexadécimal sur 64 caractères et la version doit respecter le format numérique existant.

## Tâches TDD

### 1. Modèle et validation du manifeste

**Test :** ajouter les cas version supérieure, version identique/inférieure, JSON invalide, HTTPS obligatoire, extension invalide, taille invalide, hash invalide et champs inconnus tolérés.

**Implémentation :** créer `updater.py` avec des exceptions dédiées et des dataclasses immuables. Ne pas ouvrir de processus ni écrire dans les données métier.

**Vérification :** `pytest tests/test_updater.py -q`.

### 2. Téléchargement atomique et vérification

**Test :** utiliser un serveur HTTP(S) de test local approprié ou un transport injecté ; vérifier le fichier temporaire, le hash correct, le refus d’un hash incorrect, la limite de taille et l’absence de fichier final partiel.

**Implémentation :** télécharger par blocs dans un fichier temporaire du répertoire cible, contrôler la taille et SHA-256, puis faire un renommage atomique. Refuser les redirections vers HTTP et ne jamais écrire dans `%LOCALAPPDATA%\SEL Facturation` sauf la sauvegarde prévue.

**Vérification :** tests unitaires ciblés puis suite complète.

### 3. Sauvegarde pré-installation

**Test :** préparer une base représentative, appeler le préparateur et vérifier l’existence de la sauvegarde, `PRAGMA integrity_check = ok`, puis l’absence de modification de la base courante.

**Implémentation :** réutiliser `db.backup()` dans un fichier horodaté sous `sauvegardes`, avec nettoyage des fichiers temporaires en échec.

### 4. Routes et page de contrôle

**Test :** utilisateur non connecté redirigé, réseau indisponible rendu comme état non bloquant, version disponible affichée, version déjà courante ignorée, confirmation obligatoire avant installation.

**Implémentation :** ajouter `/updates` et `/updates/check`, stocker uniquement un résultat temporaire en session, afficher un bouton « Rechercher une mise à jour » et une action d’installation distincte.

### 5. Lancement natif sécurisé

**Test :** vérifier que le pont refuse tout chemin qui n’est pas le fichier téléchargé validé, construit uniquement les arguments `/quiet /norestart` du bootstrapper, et ne lance rien si la sauvegarde ou la vérification échoue.

**Implémentation :** ajouter à `desktop.py` une méthode interne de lancement via `subprocess.Popen`; le processus d’installation sera démarré après confirmation, puis l’application se fermera proprement. Aucun shell ni concaténation de commande.

### 6. Documentation et recette Windows

**Test :** exécuter suite complète, reconstruire EXE/MSI/bootstrapper x64, vérifier le lancement de l’EXE, puis effectuer un scénario avec manifeste local : version supérieure, hash correct, sauvegarde et lancement contrôlé.

**Documentation :** décrire la configuration du manifeste, le fonctionnement hors ligne, la signature obligatoire en production et le statut `pilote non validé sur poste cible SEL`.

## Ordre d’exécution

1. Tâches 1–2 et tests unitaires.
2. Tâche 3 et tests de conservation.
3. Tâche 4 et tests HTTP.
4. Tâche 5 et tests du pont natif.
5. Tâche 6, suite complète, build et PR.

## Revue de risques

- Une URL GitHub valide mais un binaire non signé doit rester marqué non production.
- Une panne réseau ne doit jamais bloquer une ouverture locale.
- Une fermeture forcée ne doit pas intervenir avant la fin de la sauvegarde.
- Une version égale ne doit jamais réinstaller le même package.
- Un fichier téléchargé partiellement ne doit jamais être présenté à l’installeur.
