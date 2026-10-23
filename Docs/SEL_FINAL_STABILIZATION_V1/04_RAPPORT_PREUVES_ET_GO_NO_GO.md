# Format obligatoire du rapport de fin de mission Codex

Créer/mettre à jour dans **la branche technique** `Docs/SEL_FINAL_STABILIZATION_V1/IMPLEMENTATION_REPORT.md`. Le rapport ne doit jamais être une copie des assertions attendues : **consigner les résultats réellement observés** et associer chaque résultat à une preuve inspectable.

## En-tête
- Date/heure de dernière exécution, commit exact, base, numéro(s) PR, version de test (`version.py`), système Windows utilisé et configuration Python.
- Tableau diff : fonctions touchées, justification, tests associés, compatibilité/impact sur SQLite, anciennes factures, SafeUpdate.
- Origine des correctifs (P0-A, P0-B, P0-C, P1-A, P1-B, P1-C), situation avant/après avec reproduction.

## Tableau traçabilité
| ID | Avant | Après | Test commandé | Résultat | Lien preuve |
|---|---|---|---|---|---|
| FX-01 | ... | ... | ... | PASS/FAIL/... | ... |
| TR-01 | ... | ... | ... | ... | ... |
| BI-01 | ... | ... | ... | ... | ... |

Inclure **tous les IDs** du fichier `03_MATRICE_TESTS_ACCEPTATION.md`. Éviter les ellipses dans le rapport final.

## Obligations de preuve
- Sortie **exacte** de `pytest`, y compris nombres PASS/FAIL/SKIP, et lien **GitHub Actions du SHA final**.
- Extraction de texte du **PDF servi par Flask** et exemples PDF A4 anonymes sans facture SEL réelle : original USD, variante CDF, eau=0, eau positive, client ancien, paiement corrigé ou annulé.
- Tableau d'égalité : somme des lignes = sous-total, + taxes = total, - net payé = solde ; test de conversion avec quantités et devise correcte.
- SQL preuve tarifs actifs/inactifs, profil fiscal confirmé/non confirmé, migration répétée, `PRAGMA integrity_check`.
- SHA256 exact des artefacts **effectivement construits** et version Windows; si aucun build généré, l'indiquer sans inventer un lien.
- Diff des snapshots clients/factures/paiements historiques sur base de test (avant/après) et résultat des exports CSV/JSON.
- Pour les vérifications hors d'accès Codex (imprimante et PC SEL, tarifs réels, clé privée): statut `NOT_TESTED`/`BLOCKED_EXTERNAL`, **jamais** PASS.

## Méthode de décision
1. **GO revue technique** : tous P0 automatisés et audits code PASS, PR non fusionnée, CI verte et aucun danger pour données.
2. **GO pilote Windows** : tests installation/upgrade/impression réalisés sur matériel de validation avec backup et récupération éprouvés. Sinon NO-GO pilote matériel.
3. **GO facturation commerciale** : tarif de l'eau et fiscalité confirmés par SEL, matériel testé, signature interne/release policy approuvées si mises à jour prévues, validations réglementaires externes éventuellement nécessaires. Sinon NO-GO commercial.
4. Signature Windows **Authenticode non obligatoire** à cette phase, mais SmartScreen/UAC peuvent intervenir; aucune prétention de certification.
5. Un GO technique n'implique **jamais** autorisation de fusion, de release, d'installation ou de démarrage commercial.

## Message de clôture requis
Codex doit terminer par : URL PR, SHA, progrès par phase et global réel, anomalies résolues, tests exacts, démonstrations, état GO/NO-GO distinct sur les 3 axes, validations externes restantes. **Ne pas annoncer 100 %** si des exigences bloquantes restent non vérifiées.
