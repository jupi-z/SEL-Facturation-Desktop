# Mission Codex — SEL Facturation : document unique eau + électricité

**Dépôt :** https://github.com/jupi-z/SEL-Facturation-Desktop  
**Mandat :** adapter **le parcours de facturation et son PDF**, pas développer un nouveau logiciel.  
**Base observée par ChatGPT (lecture seule, le 22/10/2026) :** `baf7a88aae9696b29dff4fd45919daad861c9d7a`.  
**Autorisation de modification :** Ce dossier a été ajouté au dépôt comme documentation seulement. Aucune modification du logiciel n'a été autorisée pour cette étape. Quand le donneur d'ordre te remet explicitement ce dossier pour **exécution**, tu peux créer une branche Codex et modifier le code nécessaire **uniquement dans le périmètre ci-dessous**. Ne pas fusionner ni publier de release sans autorisation expresse.

## Résultat demandé

Un document SEL A4 élégant, sélectionnable et imprimable, réunissant **deux sections distinctes : eau et électricité**, avec leurs unités et montants propres, un **total commun**, puis un **QR de référence interne SEL** clairement séparé d'un futur QR fiscal officiel. Corriger les remarques de la facture papier : titre eau+électricité, non-répétition du type d'abonné, « eau / élec. » distingués, ligne isolée « tarif unitaire HT » supprimée mais détails préservés, emplacement QR. Ne jamais deviner l'annotation « 05 ».

### Consignes absolues

1. Commence par un audit en lecture de `main`, `AGENTS.md`, `README.md`, `invoice_pdf.py`, `domain.py`, `db.py`, `app.py`, `templates/new_invoice.html`, `templates/invoice_preview.html`, `templates/invoice_detail.html`, des exports et tests. Compare le dernier commit réellement présent avec la base ci-dessus.
2. **Respecte en priorité** `01_SPECIFICATION_NORMATIVE.md`, puis `02_TESTS_ET_RECETTE.md` ; `03_DECISIONS_A_CONFIRMER_SEL.md` contient les faits **inconnus** et `04_MAQUETTE_A4.md` la cible visuelle.
3. Implémente la présentation commune et le contrat de snapshot de manière **additive et rétrocompatible**. Les anciens décomptes électriques doivent rester lisibles, corrects et non modifiés. Les exports JSON existants doivent rester compatibles.
4. Les modes/tarifs/taxes de l'eau ne sont **pas confirmés**. Prévois l'adaptation minimale de l'entrée de facture, mais **ne permets pas d'émettre une facture combinée réelle** avec eau fictive, TVA eau supposée ou tarif d'exemple. Fournis des **fixtures synthétiques uniquement en tests** et un chemin clair d'activation après validation SEL. Les anciennes factures d'électricité restent fonctionnelles.
5. Le QR interne doit encoder une **référence documentaire**, pas un lien inexistant ou un faux QR fiscal. Ne jamais imiter la certification DGI. Réserver une zone distincte pour un futur QR fiscal officiel, invisible tant qu'aucune donnée officielle n'existe. Aucune donnée personnelle n'entre dans le QR interne.
6. N'apporte pas de refonte du dashboard, des paramètres généraux, du packaging, des mises à jour Windows, de la marque ou de Monarque Connect. Toucher seulement au moteur de PDF, à la saisie/preview, au modèle de calcul / snapshots et aux tests **strictement nécessaires**.
7. Prépare des captures comparatives/aperçus PDF avec données synthétiques, tests automatisés, audit d'impression, preuve de non-régression et rapport des décisions SEL manquantes.
8. Travaille par **branche dédiée → commits focalisés → PR non fusionnée**. Pas de merge, pas d'upload d'installeur, pas de publication commerciale tant qu'on ne valide pas ensemble le rendu final.

## Ordre d'exécution conseillé

- **Audit de départ :** calculs, règles de rendu actuel, contrat de snapshot et tests ; identifier les invariants et le nombre exact de tests de référence.
- **Spécification technique courte :** indiquer le plus petit ensemble de fichiers à modifier et les données nouvelles indispensables ; exclure le reste.
- **Tests en échec contrôlés :** rendu deux services, total, QR décodable, anciennes factures identiques sur le fond, aucune taxe eau inventée.
- **Implémentation minimale :** adapter le composant PDF puis la seule chaîne de saisie/snapshot nécessaire, sans nouvelles tables si une extension non destructive des paramètres/snapshots suffit.
- **Vérification complète :** `pytest`, rendu A4, contrôle des taxes/arrondis, test des anciennes bases, QR scanné, CI Windows si applicable.
- **PR de livraison pour revue :** description, capture PDF, compromis, limites fiscales, décisions SEL en suspens, GO/NO-GO.

## Ce qu'il ne faut pas présenter comme « terminé »

- L'émission réelle eau+électricité tant que SEL n'a pas validé un tarif et un traitement fiscal de l'eau.
- La reconnaissance officielle du QR par la DGI.
- La conformité/homologation du logiciel ou l'impression réelle sur le poste SEL sans vérification.

## Démarrer depuis la discussion

Le propriétaire joint ce dossier à Codex et lui demande : **« Exécute cette mission en respectant les limites, crée une PR de revue, montre-moi les PDFs de comparaison et tes résultats de tests, ne fusionne ni ne publie avant mon accord. »**