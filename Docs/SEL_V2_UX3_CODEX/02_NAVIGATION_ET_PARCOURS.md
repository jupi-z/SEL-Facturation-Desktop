# 02 — Navigation, écrans Figma et parcours utilisateurs

## 1. Référence du prototype

Fichier : https://www.figma.com/design/EuIpbci6IR7m6ggApo5WWm/SEL-V2-%E2%80%94-Product-Design-%C2%B7-Windows---Facturation  
Page **16**, point de départ **H00** : https://www.figma.com/design/EuIpbci6IR7m6ggApo5WWm/SEL-V2?node-id=179-1481  
Page **17**, règles UX et matrice : https://www.figma.com/design/EuIpbci6IR7m6ggApo5WWm/SEL-V2?node-id=191-20

76 écrans Figma, numérotés par famille. Leurs noms exacts et descriptions doivent être lus dans Figma, et non reconstitués à partir de cet inventaire. Les 937 liens constatés prouvent la connectivité structurelle du prototype, **pas** la validité sémantique de tous les retours ou droits. Codex doit recetter chaque lien et corriger tout lien trompeur dans l'implémentation.

### Index des écrans

| Famille | Écrans | Usage |
|---|---|---|
| `H00` | Accueil, raccourcis | Point d'entrée unique du produit |
| `A01–A09` | Répertoire utilisateurs, création, sécurité, tarifs, approbation, TVA, change, produits, audit | Administration |
| `F01–F08` | Encaissements, saisie, affectation, confirmation, facture, vérification, historique, créances | Caisse et facturation |
| `O01–O11` | Services abonnés, ajout service, compteur, relevé/correction, catalogue solaire, vente, validation, échéancier, situation client | Opérations |
| `E01–E11` | Server down, session expirée, permission, chargement, config requise, révocation, vide, erreur de valeur, pas de réseau, succès, erreur de champ | États et cas limites |
| `C01–C35` | CRUD rectificatif, actions de détail, confirmations et actions non prises en charge | Corrections métier |
| `S00` | Diagnostic serveur | Accès secondaire |

Les maquettes originales des pages 00–15 sont préservées ; la page 16 est **la** cible de l'intégration.

## 2. Navigation de premier niveau — six domaines

```text
SEL Facturation
├── Accueil                 [H00] raccourcis métier et synthèse serveur
├── Abonnés                 [C01]
│   ├── Répertoire          [C01] → Créer [C02] → Vérifier [C30] → Dossier [C03]
│   ├── Dossier             [C03] → Modifier [C04] → Vérifier [C31]
│   ├── Archivage           [C05] / Restauration [C06]
│   ├── Services            [O01] → Ajouter [O02] / Modifier [C07] → Vérifier [C32] / Statut [C08]
│   └── Compteurs           [C09] → Ajouter [O03] / correction non exposée [C10]
├── Facturation             [F07]
│   ├── Relevés             [C11] → Saisie [O04] → Vérifier [C33]
│   │                        └── Correction [O05] → Détail [C12]
│   ├── Nouvelle facture    [F05] → Aperçu [F06] → Émise [C13]
│   ├── Historique          [F07] → Détail [C13] → Annulation [C14]
│   └── État du document    [E07,E08,E11,E10]
├── Paiements               [F01]
│   ├── Journal             [F01] → Nouvelle saisie [F02] → Affectation [F03] → Confirmation [F04]
│   ├── Détail              [C15] → Contrepassation [C16] → Vérification [C34]
│   ├── Affectation ultérieure [C17]
│   └── Créances            [F08] → Détail [C18] → Avoir [C19] → Vérification [C35]
├── Solaire                 [O06]
│   ├── Catalogue           [O06] → Nouvelle vente [O07] → Vérification [O08]
│   ├── Ventes              [C20] → Détail [C21]
│   └── Échéancier optionnel [O09] → Suivi contrat [O10] / avenant indisponible [C28]
└── Administration          [A01]
    ├── Utilisateurs        [A01] → Créer [A02] / Modifier accès [A03] / Désactiver [C23]
    ├── Tarifs              [A04] → Approbation [A05] / Versions [C25] / Rectification [C29]
    ├── TVA                 [A06]
    ├── Taux de change      [A07]
    ├── Produits solaires   [A08]
    └── Journal d'audit     [A09]

Diagnostic serveur [S00] → états [E01–E11] ; accessible depuis l'indicateur réseau
```

Le rattachement d'un écran à un domaine ne signifie pas que tous les sous-domaines doivent devenir de nouvelles rubriques latérales. Préférer **menu principal simple**, puis **onglets ou actions contextuelles**, selon la densité et les droits.

## 3. État de navigation à implémenter

Modéliser le contexte de navigation, par exemple avec un routeur typé ou un état équivalent, sans casser les APIs existantes :

```ts
type Domain = 'home' | 'customers' | 'billing' | 'payments' | 'solar' | 'admin';
type WorkspaceLocation = {
  domain: Domain;
  page: string;                 // code stable et non titre affiché
  customerId?: string;          // uniquement identifiant technique, jamais nom
  invoiceId?: string;
  paymentId?: string;
  receivableId?: string;
  meterId?: string;
  serviceAccountId?: string;
  solarSaleId?: string;
  source?: { domain: Domain; page: string };
  query?: { search?: string; status?: string; page?: number };
};
```

C'est un **exemple de contrat UI**, non une API déjà implémentée. Protéger la navigation de la perte de brouillon et restaurer le focus après transition.

- Sélection du client dans `C03` → nouvelle facture `F05` : `customerId` prérempli. Même logique vers encaissements, services et relevés.
- La sélection des relevés doit rester cohérente avec l'index et la date de la facture ; ne pas forcer une navigation incohérente `O04 → situation financière`.
- `Retour` ramène à la vue source du parcours ; `Annuler` ferme le formulaire et demande confirmation de perte du brouillon, pas une mutation.
- `Accueil` retrouve les six actions rapides ; la rubrique active est visible sans exiger une deuxième recherche.
- Le passage `O08` (vente confirmée) doit proposer **paiement** ou **échéancier facultatif** ou **retour au détail**. Ne pas imposer de contrat à versements à toutes les ventes.
- Ne jamais utiliser l'état `Succès` si la réponse du serveur n'atteste pas la persistance de l'écriture.

## 4. Parcours complets et invariants

### A. Abonné : créer, corriger, archiver, restaurer

```text
C01 -> C02 -> C30 -(POST réussi)-> C03 -> C04 -> C31 -(PATCH/If-Match réussi)-> C03
                                               -> C05 -(PATCH status=ARCHIVED)-> C01[archivés]
C01[archivés] -> C06 -(PATCH status=ACTIVE)-> C03
```

Contrôles : unicité code, catégorie tarifaire, version et concurrence, détails/erreurs, pas de suppression des créances liées. Un écran de « vérification » dans Figma ne peut pas remplacer une vraie commande HTTP.

### B. Relevés

```text
C11 -> O04 -> C33 -(POST réussi)-> C11
C11 -> O05 -(POST correction append-only)-> C12 -> C11
```

Correction autorisée selon le serveur (ADMIN/SUPERVISOR), motif obligatoire, relevé remplacé conservé, consommation calculée par Rust.

### C. Facture combinée

```text
C03 ou F07 -> F05 (eau + électricité) -> POST preview -> F06
F06 -> confirmation émission -> POST issue -> C13
C13 -> PDF local | paiement | C14 -> POST void -> C13 [annulée]
```

Toujours électricité kWh + eau m³ même 0. Aucune réémission accidentelle ni recalcul de facture historique. Si facture déjà réglée/conditions non satisfaites, seul le serveur détermine si void est autorisé.

### D. Paiement et créance

```text
F01 ou C18 -> F02 -> F03 (allocations) -> F04 -> POST payments/allocate -> C15
C15 -> C16 -> C34 -> POST reverse -> C15 [contrepassation tracée]
F08 -> C18 -> C19 -> C35 -> POST credit -> C18 [avoir tracé]
```

Séparer paiements partiels, affectations, reçu, écritures inverses, montant non affecté, monnaie et soldes. La prévisualisation doit afficher les données **réellement validées par le serveur**, pas une somme JS naïve.

### E. Solaire

```text
O06 -> O07 -> POST quote -> O08 -> POST sale -> C21
C21 -> paiement comptant OU contrat/échéancier [O09/O10] OU retour
```

Les corrections de vente et les avenants de contrats actifs restent **bloqués** tant que l'API ne les prend pas en charge ; ne pas masquer cette réalité par une mutation de l'état React.

### F. Administration

```text
A01 -> A02 (nouvel utilisateur) / A03 (accès) -> C23 (désactivation confirmée)
A04/A06/A07/A08 -> C25 (versions) -> C29 (nouveau brouillon) -> A05 (approbation)
                                             -> C24 (révocation version approuvée)
```

Les valeurs approuvées sont des versions datées, pas des lignes écrasées. `A05` est une **représentation d'approbation** ; les endpoints diffèrent selon type de catalogue, à garder explicites dans l'implémentation.

## 5. Composants et règles d'interface

- Réutiliser logo vectoriel officiel et composants SEL, sans fond blanc parasite sur fond sombre.
- Mise en page Windows : repère Figma 1440×900 ; valider aussi 1280×720, zoom, scroll, tableaux longs et textes non tronqués.
- Clavier : `Tab`, `Shift+Tab`, `Escape` dans les dialogues, focus visible, labels accessibles et messages de champs invalides.
- États : Loading / Empty / Success / Error / 401 / 403 / 409 / 422 / 503 / Session expirée / Maintenance / Brouillon perdu.
- Une seule action principale par écran, action irréversible clairement libellée, confirmation après revue du risque et prévention des doubles soumissions.
- Pas d'étiquette « Enregistré » avant succès API. Libeller explicitement les données de démonstration dans les tests/captures.
- Le bouton de diagnostic serveur ne doit pas être confondu avec une action de facturation hors connexion.
