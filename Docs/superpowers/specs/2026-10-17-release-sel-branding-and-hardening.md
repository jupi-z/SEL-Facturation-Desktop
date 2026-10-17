# SEL Facturation — Spécification de livraison et de durcissement

## Objectif

Préparer une version Windows livrable de SEL Facturation Desktop pour une utilisation réelle par la Société d’Énergie du Lualaba, avec une identité SEL dominante, une attribution développeur discrète et des garde-fous de données, de mise à jour et d’installation vérifiables.

## Identité et mentions

- L’application métier, les écrans de travail, les PDF et les documents clients restent présentés comme ceux de la **SOCIÉTÉ D’ÉNERGIE DU LUALABA**.
- Les logos MonarQue Group et LTEX n’apparaissent pas sur les factures, les tableaux de bord métier ou les documents destinés aux abonnés.
- Les logos fournis par l’utilisateur sont conservés sans recadrage ni déformation et utilisés uniquement dans une page ou un panneau « À propos / Mentions légales ».
- Le shell de l’application peut afficher un footer discret et non invasif : `Designed and secured by LTEX · MonarQue Group`.
- La page légale distingue clairement le propriétaire métier SEL du développeur : le logiciel est développé par le pôle informatique LTEX de MonarQue Group S.A.S.
- Les données publiques utilisées pour MonarQue Group sont : MonarQue Group S.A.S., société par actions simplifiée de droit congolais, RCCM `CD/KWZ/RCCM/26-B-00523`, ID national `14-14300-N00001Y`, siège `N° 900, Avenue Bunkeya, Quartier Mutoshi, Commune de Manika, Ville de Kolwezi, Province du Lualaba, République Démocratique du Congo`, email `contact@monarquegrp.com`, téléphones `+243 840 855 547` et `+243 971 666 441`, site `https://monarquegrp.com/`.
- LTEX est présenté comme un pôle informatique, sans lui attribuer une personnalité juridique, un RCCM ou des coordonnées non fournis.
- Les mentions existantes indiquant que l’application est locale, que les données restent sur l’ordinateur et que les montants ne sont pas authentifiés par la DGI sont conservées.

## Données et sauvegardes

- Les sauvegardes quotidiennes, les snapshots pré-mise à jour et les exports manuels sont des classes distinctes.
- La rétention configurable s’applique uniquement aux sauvegardes quotidiennes.
- Un snapshot pré-mise à jour ne peut pas être supprimé par le nettoyage quotidien.
- Toute sauvegarde écrite est atomique et validée par `PRAGMA integrity_check` avant remplacement.
- Les migrations SQLite restent additives et ne suppriment jamais les données métier.

## Installation et mise à jour

- WiX Burn reste le seul installateur officiel supporté ; Inno Setup reste historique.
- Un installateur inférieur à la version présente est refusé avant modification des fichiers ou des données.
- Une même version déclenche une réparation explicite ou un message clair, sans seconde entrée ambiguë.
- Une version supérieure conserve la base, crée un snapshot vérifié avant migration/remplacement et ne supprime jamais le dossier de données SEL.
- La détection s’appuie sur les enregistrements d’installation supportés et sur un marqueur local contrôlé pour les copies portables déjà lancées ; aucune promesse impossible de détection d’une copie arbitraire jamais exécutée ne doit être faite.
- Le contrôle de mise à jour réseau doit rester HTTPS, borné en taille, vérifié par hash et signature, et ne doit jamais exécuter directement un téléchargement non validé.
- La publication d’un artefact doit dériver son tag de `APP_VERSION` et refuser les artefacts non signés.

## Version et livrables

- La version de livraison candidate est `0.3.8`.
- Le build doit produire l’EXE, le MSI et le bootstrapper WiX avec des noms cohérents.
- La signature Authenticode et la recette sur le poste SEL sont des gates externes : tant qu’elles ne sont pas réalisées, le statut est **pilote non validé sur poste cible**.

## Hors périmètre

- Aucun remplacement de Python, Flask, SQLite, pywebview, ReportLab ou WebView2.
- Aucun ajout de QR code, signature DGI, numéro fiscal fictif ou certification fiscale.
- Aucun logo MonarQue Group ou LTEX dans le PDF client.
- Aucun effacement ou réinitialisation destructive des données existantes.
