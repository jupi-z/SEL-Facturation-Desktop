# Protocole cryptographique SafeUpdate V1 — manifeste signé, EXE Windows non signé

## Principe de confiance
Cette V1 n'exige **aucun** certificat de signature de code Microsoft / Authenticode. Elle nécessite une **paire Ed25519 propre à SEL** servant uniquement à authentifier les métadonnées de distribution. La clé publique vérificatrice est intégrée au binaire SEL de la **première** version compatible; sa provenance est donc attestée par une installation initiale manuelle contrôlée. La clé privée **NE DOIT JAMAIS entrer dans Git, ni dans une archive de release, un binaire client ou un rapport d'exécution**.

Cryptographie standard : `cryptography.hazmat.primitives.asymmetric.ed25519` (API publique documentée). Aucun algorithme propriétaire. Pour vérifier une signature, la méthode `public_key.verify(sig, message)` échoue si les octets ne correspondent pas.

## Fichiers distribués
- `updates/latest.json` : **octets UTF-8 exacts** (aucune ré-sérialisation avant vérification) ; taille max 128 KiB.
- `updates/latest.sig.json` : petite enveloppe détachée contenant `{"schema":1,"key_id":"sel-update-2026-01","signature_b64":"..."}`. Le `key_id` sélectionne une clé publique épinglée et ne donne aucun pouvoir à la réponse réseau.
- `SEL-Facturation-Setup-<version>.exe` : **EXE unsigned Authenticode**, de provenance applicative prouvée par hash signé et vérification dans l'updater SEL.
- Chaque changement des deux fichiers de manifeste doit être **atomique dans un même commit** sur la branche choisie; attention aux caches CDN : en cas de décalage entre .json et .sig, réessayer de manière bornée puis reporter sans rien lancer.
- Les données JSON avec clés dupliquées sont refusées; nombres de montants / dates de facturation ne sont pas concernés et ne doivent jamais influencer la politique d'update.

## Exemple purement schématique (ne PAS publier comme release valide)
```json
{
  "schema_version": 2,
  "product": "SEL-Facturation",
  "channel": "preview",
  "version": "0.3.25",
  "sequence": 25,
  "created_at": "2026-10-22T12:00:00Z",
  "expires_at": "2026-11-22T12:00:00Z",
  "min_supported_version": "0.3.25",
  "package_type": "bootstrapper",
  "windows_code_signed": false,
  "url": "https://github.com/jupi-z/SEL-Facturation-Desktop/releases/download/v0.3.25/SEL-Facturation-Setup-0.3.25.exe",
  "release_url": "https://github.com/jupi-z/SEL-Facturation-Desktop/releases/tag/v0.3.25",
  "size_bytes": 42900000,
  "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "notes": "Améliorations et correctifs de SEL Facturation"
}
```
**Exemple fictif :** valeurs `version`, `sha256`, `sequence`, `size_bytes`, dates et URL ci-dessus **ne doivent jamais être intégrées comme métadonnées réelles**. La release builder doit les calculer à partir du fichier existant et approuvé. Un `min_supported_version` supérieur au client installé doit produire « installation de transition requise », et non une tentative d'upgrade silencieuse.

## Format signé
**MESSAGE** = `bytes` du fichier UTF-8 `latest.json` téléchargé. **SIGNATURE** = Ed25519.Sign(private_key, MESSAGE). Signature encodée Base64 dans `latest.sig.json`, `key_id` figé par le tableau de clés publiques embarquées. Ce protocole évite les divergences JSON canonicalization. Les tests vérifient indépendamment une modification d'un seul octet, reformatage, ajout de clé, substitution d'URL, changement hash ou expiration.

Ordre obligatoire :
1. Définir endpoint exact connu du **programme client** (HTTPS). Charger la signature et le manifeste en byte-array borné, limiter redirects/timeout et refuser schemes/file/loopback/private intrus.
2. Décoder signature, trouver **une** clé publique épinglée, vérifier Ed25519 sur bytes exacts **avant toute décision d'installation**.
3. Décoder JSON strict (UTF-8, JSON object unique, pas de NaN/Infinity ni clés dupliquées), valider schéma/contraintes, produit, version, séquence, expiration et canal autorisé.
4. Vérifier monotonicité : garder en local dernier `sequence` accepté et dernière version installée; refuser downgrade, replay et switch de canal non consenti. Les compromis locaux de l'OS ne sont pas couverts par cette mesure.
5. Télécharger le **fichier EXE uniquement** selon la URL signée; taille max 512 MiB, HTTPS, fichier temporaire privé et timeout. Hash SHA-256 streaming; taille et hash égaux à la déclaration signée; sauvegarder dans staging atomiquement.
6. Au moment de lancer : revalider hash du fichier staging + politique, verrou de transaction, explicit consent, DB saine / sauvegarde. Aucune exécution des champs `notes` / `url` comme commande shell.
7. Installer avec arguments codés en dur, sous droits actuels, puis contrôler le résultat réel.
8. Consigner seulement `key_id`, version, hash public, temps et état ; jamais clé privée, cookies, données de facturation ou clés de session.

## Menaces et réponses
| Menace | Réponse obligatoire |
|---|---|
| Serveur / CDN / dépôt compromis | Signature Ed25519 non forgeable sans clé privée |
| Manifeste signé mais EXE modifié | Hash/size en manifeste signé; vérification juste avant exécution |
| Manifeste ancien / downgrade | Version monotone + numéro de séquence + expiration (avec tolérance horloge documentée) |
| Clé privée volée | Procédure de révocation/rotation et distribution d'un nouveau client racine approuvé; arrêt du canal compromis |
| Key ID arbitraire | Liste autorisée intégrée au client, rejet de toute clé inconnue |
| Attaques TOCTOU / écriture locale concurrente | Staging privé, verrou transaction, relecture hash juste avant lancement et maîtrise ACL; reconnaître limites face à utilisateur local malveillant |
| CDN renvoie ancien JSON et nouvelle signature | Réessai borné, arrêt sans installation |
| GitHub indisponible | Application continue hors ligne |
| Poste contaminé ou administrateur hostile | **Hors du modèle de menace** : aucune signature applicative ne peut protéger un OS déjà compromis |

## Gestion des clés
- Fournir un script de génération **hors dépôt** depuis un poste contrôlé; cryptographie Ed25519 via `cryptography`; stockage privé PKCS8 protégé par phrase secrète forte + copie de secours chiffrée **hors ligne**. Le script ne doit pas afficher la clé en clair ni permettre par défaut son écriture dans le repo.
- Publier seulement la clé publique (base64) et son empreinte / `key_id`. Documenter qui détient la clé, la procédure d'approbation d'une release et la rotation.
- Pour automatiser en CI ultérieurement, utiliser GitHub environment protégé + approbation + secret de moindre privilège. Pas d'acceptation automatique d'une PR non relue à un job possédant la clé.
- Rotation préférée : embarquer ancienne et future clés publiques dans une version de transition signée par l'ancienne; activation future datée/consentie; retirer l'ancienne après fenêtre de migration. Si clé compromise, stopper publication et exiger une transition de confiance manuelle sur les anciens clients.
- Les tests utilisent **une paire de test distincte** à régénérer, sans clé privée de production.

## Limite de garantie
Ed25519 **n'est pas Authenticode**. Windows ne lui accorde aucune réputation SmartScreen particulière. Une politique Smart App Control peut bloquer le lancement, et le projet doit prévoir un recours légitime à la gestion du poste SEL sans essayer de contourner l'OS.
