# Audit UX — Nouveau décompte de consommation

Date : 17 octobre 2026
Surface : `/invoices/new`
Preuve visuelle : [01-new-invoice.png](01-new-invoice.png)

## Périmètre et objectif

L’utilisateur doit sélectionner un abonné, saisir la période, la consommation en kWh, l’échéance et une observation facultative, puis contrôler une prévisualisation avant l’émission définitive.

## Constat

L’écran dédié est justifié pour ce parcours. Le serveur sépare déjà la saisie, la prévisualisation et l’émission immuable : cette séparation protège contre une facture créée par erreur et rend le parcours explicable.

En revanche, l’écran utilise presque toute la largeur disponible alors que les champs occupent une zone plus étroite. Le bouton d’action s’étend sur toute la ligne, ce qui dilue sa priorité. La saisie et la prévisualisation forment aussi un parcours en deux étapes ; un modal unique rendrait cette séquence plus fragile, surtout avec les erreurs, les noms longs et les petits écrans.

## Évaluation par étape

1. **Entrée dans la saisie — bon** : le menu actif, le titre et le libellé « Nouveau décompte » rendent la tâche identifiable.
2. **Saisie — acceptable** : les champs sont regroupés logiquement et étiquetés, mais la largeur et les espaces inutilisés augmentent le balayage visuel.
3. **Contrôle avant émission — bon principe** : le bouton annonce une vérification avant création ; la prévisualisation protège les données historiques.
4. **Retour et correction — à renforcer** : le lien de retour existe, mais le brouillon devrait être restitué avec ses valeurs pour éviter une ressaisie après la prévisualisation.
5. **Accessibilité et responsive — à vérifier sur poste** : les labels sont associés aux champs dans le code, mais il faut encore tester le focus clavier, les erreurs, le zoom 200 %, les petits écrans et les listes d’abonnés longues.

## Modal ou écran dédié ?

Un modal serait pertinent pour une action courte de trois champs ou moins, sans étape de contrôle. Ici, il augmenterait les risques : formulaire trop haut, défilement interne, fermeture accidentelle, gestion moins claire du retour depuis la prévisualisation et comportement plus fragile au clavier.

## Recommandation

Conserver l’écran dédié, mais le transformer en **espace de saisie focalisé** :

- limiter la largeur utile du formulaire à environ 1 000–1 200 px ;
- garder une grille claire : abonné/période, consommation/échéance, observation pleine largeur ;
- remplacer le bouton pleine largeur par une action alignée à droite, libellée « Prévisualiser le décompte » ;
- afficher après sélection l’abonné, son type et le tarif appliqué en lecture seule ;
- marquer explicitement les champs obligatoires et donner une explication si l’émission est bloquée par une configuration manquante ;
- conserver la prévisualisation comme étape dédiée ;
- préremplir la saisie lorsque l’utilisateur revient depuis la prévisualisation.

Une action rapide peut être ajoutée depuis le tableau de bord ou l’historique, mais elle doit simplement naviguer vers cet écran. Un second formulaire dans un modal créerait deux parcours de calcul à maintenir et risquerait de produire des comportements divergents.

## Limites

Cet audit s’appuie sur la capture fournie et sur la structure actuelle des templates/routes. Il ne constitue pas une validation complète du clavier, du lecteur d’écran, du zoom, du poste Windows ou de l’imprimante SEL.
