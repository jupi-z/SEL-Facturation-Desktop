# Rapport d’implémentation — SEL SafeUpdate V1

Date du rapport : 22 octobre 2026  
Branche : `monarque/sel-safeupdate-v1`  
Base : `origin/main` (`d5f4eb2cccc76618281da41c2a3e6a3b3f74af03`)

## Résultat

Le mécanisme SafeUpdate V1 est implémenté sur une branche dédiée, sans fusion
dans `main`, sans publication de release et sans déploiement sur un poste SEL.
Le chemin d’installation est fermé par défaut tant que la clé publique Ed25519
de production n’a pas été fournie et vérifiée hors bande.

## Changements

- `safeupdate_crypto.py` authentifie les octets exacts de `latest.json` avec
  Ed25519, refuse les doublons JSON, constantes non finies, encodages invalides,
  schémas inconnus, URL dangereuses, expiration, downgrade, mauvais canal et
  régression de séquence; il vérifie aussi taille et SHA-256 du package.
- `safeupdate_keys.py` contient une table de confiance vide volontairement.
  Aucun secret, clé privée ou clé de test n’est embarqué.
- `update_client.py` récupère le manifeste et `latest.sig.json`, borne les
  réponses, refuse les redirections non identiques, télécharge dans une
  transaction utilisateur privée, expose les octets reçus et permet
  l’annulation.
- `update_transaction.py` conserve une preuve atomique (manifest bytes,
  signature, séquence, clé, hash et taille) et revalide le package juste avant
  toute installation.
- `safeupdate_coordinator.py` sépare le réseau et la sauvegarde du thread Flask,
  sérialise les opérations et publie les états `checking`, `available`,
  `downloading`, `verifying`, `backup`, `ready`, `failed`, `cancelled` et
  `recovery_required`.
- `safeupdate_supervisor.py` reçoit uniquement un identifiant opaque, attend la
  fermeture du processus parent, revalide la transaction et lance le
  bootstrapper avec la liste fixe `[/norestart]`, sans shell, élévation ou
  contournement SmartScreen/UAC.
- `desktop.py`, `app.py`, `templates/updates.html` et `static/desktop.js` sont
  raccordés au flux asynchrone; l’interface distingue la validation du
  manifeste de la signature Windows et ne fabrique pas de pourcentage
  d’installation.
- `tools/generate_signed_update.py`, `tools/verify_signed_update.py` et
  `tools/generate_safeupdate_key.py` fournissent la chaîne hors ligne. Le
  générateur refuse une clé privée placée dans le dépôt.
- Le workflow Windows exécute les tests SafeUpdate et ne publie plus de release
  automatiquement dans cette branche; les artefacts de build restent des
  artefacts CI de validation.

## Vérifications effectuées

Les tests cryptographiques couvrent les CR-01 à CR-17 applicables avec des clés
éphémères de test : signature valide d’un EXE non signé Windows, mutation d’un
octet, mauvaise signature, key-id inconnu, signature absente, JSON dupliqué ou
non UTF-8, constantes JSON, URL non sûre, expiration, downgrade, replay de
séquence, canal non autorisé, hash/taille incohérents, staging modifié et
annulation.

Les tests applicatifs couvrent aussi la sérialisation du coordinateur, la
sauvegarde obligatoire, l’absence de confiance dans le manifeste historique
`updates/latest.json`, le pont natif opaque et la liste d’arguments du
superviseur.

Commandes de preuve à rejouer sur la branche :

```text
python -m pytest -q
python -m compileall -q app.py desktop.py safeupdate_crypto.py safeupdate_keys.py safeupdate_coordinator.py safeupdate_supervisor.py update_client.py update_transaction.py tools
git diff --check
```

Résultat local final : `256 passed in 99.56s` dans le venv de build Windows.
`build-windows.ps1` a aussi terminé la compilation PyInstaller, MSI WiX et
bootstrapper/preflight sur Windows 11 x64. Les artefacts locaux de cette
vérification ne sont pas publiés :

```text
dist/SEL-Facturation.exe
SHA-256 6FCD1C5CA3BF4D48143A34DC142C30811D21721DEA2B47A514700E170B94B91D

dist/installer/SEL-Facturation.msi
SHA-256 FCC74ECF9B03AB8882086A4B42DF62FD842C611DF056C844CCC6470388925C1A

dist/installer/SEL-Facturation-Setup-0.3.24.exe
SHA-256 8F1ACE27EFBC50E5561D71DF7225817A08704B4D376D8A7DDAEC7233635C8B6E
```

Aucun essai d’installation ou de mise à jour n’a été déclaré PASS sur le
matériel SEL.

## Décisions et limites restantes

1. SEL doit fournir une clé Ed25519 de production, son `key_id` et son
   empreinte vérifiée par une seconde voie. Ils devront être ajoutés à
   `safeupdate_keys.py` dans une version de transition approuvée.
2. La transition depuis 0.3.24 nécessite une installation manuelle approuvée;
   le simple remplacement du manifeste distant ne peut pas rendre les anciens
   exécutables compatibles.
3. Il faut publier ensuite, de façon atomique et après approbation, une release
   immutable, `latest.json` et `latest.sig.json`. Cette intervention ne les
   publie pas.
4. Les recettes W-01 à W-12, WebView2 absent, SmartScreen, interruption
   d’alimentation, deux mises à jour successives et la conservation des
   factures/paiements doivent encore être exécutées sur des machines Windows
   de validation. Le pilote reste **non validé sur le poste cible SEL**.

## Statut de livraison

- Revue technique de branche : GO conditionnel après CI Windows et revue PR.
- Canal de production : NO-GO tant que la clé de production et la recette
  Windows cible ne sont pas validées.
- Fusion `main`, release GitHub et déploiement SEL : non effectués.
