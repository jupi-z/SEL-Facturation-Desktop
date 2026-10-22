# Maquette fonctionnelle — facture SEL eau + électricité

**Document cible :** A4 portrait, texte vectoriel ReportLab sélectionnable, identité SEL existante (bleu dominant, accent jaune du logo, fonds sobres), pas d'ombres, pas de superpositions, pas de surcharge.

> La structure ci-dessous n'est **pas** une facture déjà délivrée. C'est un guide spatial pour Codex. Les chiffres doivent provenir des snapshots ; toute ligne d'eau sur un exemple est exclusivement synthétique et doit être identifiée comme telle pendant la revue.

```text
┌───────────────────────────────────────────────────────────────────────────┐
│ [LOGO SEL]  SOCIÉTÉ D'ÉNERGIE DU LUALABA      DÉCOMPTE DE CONSOMMATION   │
│             adresse, RCCM, NIF               EAU & ÉLECTRICITÉ          │
│             contacts                         N° SEL-....  |  Date      │
├───────────────────────────────────────────────────────────────────────────┤
│ ABONNÉ / CONTRAT                        PÉRIODE / POINT DE SERVICE     │
│ Nom, code client, contrat                Centre, point de service      │
│ Adresse, type (une seule fois), NIF      Période, date d'échéance      │
├───────────────────────────────────────────────────────────────────────────┤
│ ÉLECTRICITÉ                                                              │
│ Mode : consommation directe OU relevé réel et coefficient                │
│ Désignation           Quantité kWh    PU HT /kWh       Montant HT        │
│ Consommation ...      [donnée]        [tarif validé]  [calculé]         │
│ Sous-total électricité HT                                    [calculé]   │
├───────────────────────────────────────────────────────────────────────────┤
│ EAU                                                                       │
│ Mode : m³ au compteur / direct OU forfait, selon décision SEL            │
│ Désignation           Quantité m³     PU HT /m³       Montant HT        │
│ Consommation ...      [donnée]        [tarif validé]  [calculé]         │
│ Sous-total eau HT                                            [calculé]   │
├───────────────────────────────────────────────────────────────────────────┤
│ RÉCAPITULATIF GLOBAL                                   [devise unique] │
│ Électricité HT                                              [montant]    │
│ Eau HT                                                      [montant]    │
│ Taxes électricité (base, taux si applicable)                [montant]    │
│ Taxes eau (base, taux si applicable)                        [montant]    │
│ Frais autorisés / non taxable selon configuration           [montant]    │
│ ═══════════════════════════════════════════════════════════════════════   │
│ TOTAL À PAYER                                               [montant]    │
├───────────────────────────────────────────────────────────────────────────┤
│ PAIEMENT & ÉCHÉANCE                     RÉFÉRENCE DU DÉCOMPTE           │
│ Date limite, modalités                   ┌──────────────┐               │
│ Déjà payé, solde                         │  QR INTERNE  │               │
│                                         └──────────────┘               │
│                                         Référence interne SEL           │
├───────────────────────────────────────────────────────────────────────────┤
│ Mentions entreprise complémentaires ; référence du document ; Page x/y  │
└───────────────────────────────────────────────────────────────────────────┘
```

**Règles de densité :** l'en-tête et les deux services doivent être lisibles sur papier A4. Prévoir une pagination lorsque la facture devient longue ; le QR et le total doivent rester avec la synthèse de règlement, pas isolés dans une page quasi vide. Ne pas déformer le logo. Éviter la duplication de « résidentiel » et la ligne séparée « tarif unitaire HT » du récapitulatif ; chaque prix unitaire reste dans son tableau.

**Cas où l'eau est indisponible :** aucun chiffre fictif ; bloquer l'émission de nouvelle facture combinée. Les anciens PDF ne sont pas réécrits. Les simulations internes peuvent utiliser des fixtures clairement identifiées dans les tests.

**QR fiscal futur :** pas de pseudo-QR DGI ni cadre « DGI » vide sur la facture client. Prévoir une extension du renderer qui pourra plus tard intégrer un contenu officiel séparé lorsque la procédure aura été validée.