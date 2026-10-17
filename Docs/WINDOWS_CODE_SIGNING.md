# Signature Windows SEL

Les versions publiques doivent être signées avec une identité de publication SEL reconnue par Windows. Un certificat auto-signé ne convient pas pour une distribution réelle.

## Préparation

La workflow Windows accepte `sign_release=true` uniquement pour une publication manuelle. Elle attend deux secrets GitHub Actions :

- `SEL_CODESIGN_PFX_BASE64` : certificat Authenticode SEL au format PFX encodé en Base64 ;
- `SEL_CODESIGN_PFX_PASSWORD` : mot de passe du PFX.

Le certificat doit être émis par une autorité publique reconnue. Microsoft Artifact Signing est l’option recommandée pour une distribution hors Microsoft Store ; un certificat OV public est une alternative.

## Publication

1. Configurer et vérifier l’identité SEL auprès du prestataire de signature.
2. Ajouter les deux secrets dans les paramètres GitHub du dépôt.
3. Déclencher `Build Windows Desktop` avec `publish_release=true` et `sign_release=true`.
4. Vérifier dans les logs que les trois artefacts portent une signature valide.
5. Distribuer le bootstrapper `SEL-Facturation-Setup-<version>.exe`, qui embarque déjà le runtime WebView2 hors ligne.

La signature n’élimine pas nécessairement le premier avertissement SmartScreen : la réputation du nouvel éditeur et du fichier se construit progressivement. Elle permet toutefois à Windows d’identifier clairement SEL comme éditeur et évite la situation actuelle de fichier non signé.
