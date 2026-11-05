# SEL Facturation — client Windows connecté

SEL Facturation V2 est une application Windows pour la **Société d’Énergie du Lualaba (SEL)**. Toute authentification, lecture ou écriture métier nécessite une connexion au serveur central SEL. En cas d’indisponibilité du serveur ou d’Internet, les opérations métier sont bloquées jusqu’au rétablissement de la connexion.

## Produit pris en charge

Le client Windows pris en charge se trouve dans [`v2/windows-app`](v2/windows-app). Il utilise Tauri et l’API Rust centrale. La session utilisateur est conservée de manière protégée par Windows ; les clés techniques de reprise des requêtes ne constituent pas une base métier locale. Les documents PDF sont produits côté poste à partir des données confirmées par le serveur.

Les anciennes applications V1 et le lanceur PyWebView/SQLite V2 ne sont plus distribuables. Leurs lanceurs et scripts de build sont neutralisés. Les sources, contrats et migrations historiques demeurent dans le dépôt uniquement pour la traçabilité, l’analyse et les migrations contrôlées ; ils ne sont pas inclus dans le client Windows connecté.

## Développement du client Windows

Prérequis : Node.js, Rust avec la cible Windows MSVC et l’outillage Tauri requis par Windows.

```powershell
cd v2/windows-app
npm ci
npm run build
npm run tauri -- build --bundles msi,nsis
```

Les paramètres centraux du client sont définis dans sa configuration Tauri. Une session valide et un serveur SEL disponible sont requis pour utiliser les fonctionnalités métier. Le client ne démarre aucun agent local et n’embarque aucune base SQLite, outbox ou file de synchronisation métier.

## Documentation

- [Guide du client V2 et de sa configuration](v2/README.md)
- [Rapport d’implémentation et limites de validation](Docs/SEL_V2_BACKEND/IMPLEMENTATION_REPORT.md)
- Les spécifications V1/V2 et les anciens contrats de synchronisation sont conservés comme documents historiques ; ils ne décrivent pas le comportement du client Windows actuel.

## Validation et distribution

Un build local ne constitue pas une recette. Les installateurs ne doivent être distribués qu’après réussite du workflow Windows, vérification de leurs empreintes, contrôle de l’authentification contre le serveur et recette sur un poste représentatif de SEL. Le client n’est pas utilisable hors connexion.
