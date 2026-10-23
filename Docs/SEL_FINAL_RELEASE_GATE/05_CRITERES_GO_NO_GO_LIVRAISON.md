# Critères de validation et autorisation de livraison

## GO-1 : prêt à la revue technique (fusion à APPROUVER PAR LE PROPRIÉTAIRE)

Les conditions ci-dessous sont cumulatives :
- [ ] P0-01 total CDF corrigé et vérifié par calcul explicite et test HTTP/PDF; pas de seconde multiplication.
- [ ] P0-02 tarifs inactifs, expirés ou non approuvés non utilisés; sélection datée traçable.
- [ ] P0-03 montant/prix/devise par service cohérents et deux services obligatoires (dont 0 m³).
- [ ] Les correctifs P1 n'écrasent pas le statut physique du raccordement, ne facturent pas par oubli le 0 prérempli et ne réécrivent pas les factures historiques.
- [ ] La suite entière est verte, la dernière CI Windows du HEAD est verte (pas juste une version précédente).
- [ ] Les tests de conversion incluent **PDF extrait**, *valeurs numériques finales*, lignes, sous-totaux et soldes, pas seulement un test unitaire de `convert_amount`.
- [ ] PDF A4 USD et CDF réellement inspectés en images/rendu ; QR interne documentaire, pas fiscal.
- [ ] Paiements, corrections, annulations, exports JSON v1/CSV et snapshots anciens inchangés.
- [ ] Installer x64 compilé avec WebView2, tests de packaging réussis; aucun certificat Authenticode de SEL fictif.
- [ ] SafeUpdate reste fail-closed en l'absence de clé publique Ed25519 de production.
- [ ] Rapport traçable avec SHA/artefacts/liens, PR non fusionnée.

Si une condition manque : **NO-GO pour fusion/revue finale** ; si seulement un test environnemental non obligatoire sur PC cible manque, indiquer `GO-1 conditionnel, GO-2 bloqué`.

## GO-2 : pilote Windows SEL (distinct du GO-1)

En plus de GO-1 :
- [ ] Validation SEL du prix eau au m³, catégories, dates, devise, taxes/assiettes/exemptions si applicables, source/document ; les paramètres de production sont approuvés.
- [ ] Installation sous compte Windows ordinaire d'un PC propre; lancement WebView2, impression physique A4 et export PDF réels ; variation 0 m³ et >0 m³.
- [ ] Upgrade avec données de test représentatives sur le poste de recette : backup avant, facture/paiement après, pas de perte de DB.
- [ ] Deux factures comparatives imprimées (eau=0 et eau positive) acceptées par SEL; prix/TVA vérifiés par le responsable habilité.
- [ ] Vérification SmartScreen/UAC/Defender sur PC cible ; aucune promesse d'absence absolue d'intervention Windows sur unsigned.
- [ ] Politique de sauvegarde externe / restauration testée ou procédure approuvée.
- [ ] Autorisation de déploiement par le propriétaire puis SEL.

## GO-3 : mises à jour autonomes SafeUpdate (distinct du GO-2)

- [ ] Vraie **clé publique Ed25519 de production** provisionnée dans une version bootstrap approuvée, fingerprint vérifiée hors GitHub, clé privée protégée hors dépôt.
- [ ] Release signée **au niveau manifeste** et paquet SHA256 vérifiés sur serveur réel.
- [ ] Mise à jour end-to-end sur poste Windows avec deux versions successives, DB intacte, relance et recovery.
- [ ] Cas offline, SmartScreen et absence de signature Windows traités honnêtement; pas de bypass.
- [ ] État du canal publié `latest.json` **et** `latest.sig.json` cohérent et validé; l'ancien `latest.json` 0.3.24 ne suffit pas.

## Autorité de validation
Seul le propriétaire décide de fusionner, créer/publier une release ou déployer. La CI et Codex rendent un avis motivé, pas une autorisation automatique. Si l'eau n'est pas officiellement tarifée, **une facture avec volume positif ne doit pas être émise**, même si le moteur fonctionne dans des tests synthétiques.

## Format de conclusion exigé
```
HEAD audité:
PR finale:
Version candidate:
Tests unitaires/HTTP: n PASS / n FAIL / n SKIP
Windows CI: URL + résultat
PDF synthétiques: chemin/artefact + contrôle visuel
P0 ouverts: [...]
P1 ouverts: [...]
GO-1 Revue technique: GO / NO-GO / CONDITIONNEL + preuves
GO-2 Pilote SEL: GO / NO-GO + preuves/manquants
GO-3 SafeUpdate prod: GO / NO-GO + preuves/manquants
Décisions exclusives SEL: [...]
Publication/tag/fusion effectuée ? NON
```

**Ne pas annoncer le projet 100% terminé au plan métier ni une conformité fiscale DGI si la preuve n'existe pas.**
