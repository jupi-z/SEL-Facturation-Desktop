# Tests obligatoires — SEL Facturation 0.3.32+ (mission #49)

La **preuve** prime sur la déclaration « tests réussis ». Codex doit attribuer `PASS / FAIL / BLOCKED_EXTERNAL / NOT_TESTED` à chaque cas, la preuve de sortie/commande/SHA/test. Créer des tests qui **échouent avant fix** lorsque le défaut est encore présent; les conserver dans la suite de régression.

## A. Conversion et argent — P0
| ID | Scénario d'essai, attendu |
|---|---|
| FX-01 | Émettre facture USD synthétique 100 kWh à 0,095 USD/kWh + TVA électricité 16 %, eau 0 m³ **sans** tarif eau. Total source **11,02 USD**. Appeler la **route HTTP** PDF CDF avec taux synthétique 2 360; montant affiché correct **26 007,20 CDF** et non un second produit; client/snapshot USD inchangés. |
| FX-02 | Eau positive synthétique **5 m³ à 0,40 USD/m³**, fiscalité eau synthétique confirmée 0 % et électrique identique; sous-total élec 9,50 USD, TVA élec 1,52 USD, eau 2,00 USD, total **13,02 USD**, conversion CDF à 2 360 : **30 727,20 CDF**. Valeurs synthétiques **uniquement dans les tests**, pas dans les données réelles SEL. |
| FX-03 | Chaque ligne `ELECTRICITY`/`WATER`, prix unitaire étiqueté CDF/kWh ou CDF/m³, montant brut, TVA, sous-total, total et solde se réconcilient; **quantités inchangées**. Prix unitaire peut porter plus que 2 décimales si nécessaire pour garder précision, mais la facture doit expliciter la politique d'arrondi. |
| FX-04 | Paiement partiel 3,00 USD; pour FX-01 solde CDF = (11,02−3,00)×2 360 = **18 927,20 CDF** avant éventuels écarts de rounding; vérifier net paiements et contre-écriture. |
| FX-05 | Taux manquant, égal à zéro, `NaN`, négatif, hors dates autorisées, taux tamper via URL, devise initiale déjà CDF : politique claire/rejet contrôlé, sans HTTP 500; taux affiché avec source/date/justification. |
| FX-06 | Export CSV/JSON du document source USD inchangé après génération de variante CDF; PDF original USD puis CDF puis USD stable; QR interne fait référence à l'original et non à un total provisoire contradictoire. |
| FX-07 | Audit de code : aucune double conversion et `calc_snapshot` sérialisé **après** transformation complète de la copie d'affichage, ou une approche atomique équivalente. Test sur le **PDF réellement rendu**, pas seulement `render_invoice()` avec un faux snapshot déjà converti. |
| FX-08 | Arrondis et total réconciliés sur tarifs fractionnaires, eau 0, petite quantité, montants grands, taux CDF variés; zéro float binaire pour montants. |

> **Précision sur la régression observée** : dans l'ancienne PR #47, `calc['total']` est multiplié deux fois dans la variable Python, mais la sérialisation du snapshot PDF se fait **avant** la seconde multiplication et avant conversion des sous-postes. Le test doit constater les **sorties effectives** et l'état des objets, ne pas considérer « PDF 61 376 992 CDF » comme acquis par simple inspection.

## B. Tarifs et fiscalité — P0
| ID | Scénario |
|---|---|
| TR-01 | Tarif eau `active=0`, `confirmed=1` : non utilisable pour nouveau volume d'eau positif; pas de facture silencieuse. |
| TR-02 | Tarif électricité `active=0` : non sélectionné pour nouveau document; vue paramètres et calcul cohérents. |
| TR-03 | Deux tarifs datés (ancien actif / récent inactif) : appliquer la politique normative explicitée, avec test absence d'utilisation de tarif révoqué. |
| TR-04 | Tarif futur actif : ne s'applique pas à une période antérieure; période d'effet exacte sélectionnée, mois/multi-années. |
| TR-05 | Eau positive sans prix ou sans profil fiscal *actif et confirmé* : refus avec erreur utile, aucune facture ni numéro consommé. |
| TR-06 | Eau zéro sans barème eau : ligne `0 m³`, total eau `0,00`, aucune taxe, exemption ou tarif inventé; **pas d'affirmation d'exonération**. |
| TR-07 | Eau et électricité avec monnaies tarifaires différentes : refuser mélange silencieux, ou conversion explicitement autorisée avec même justificatif de taux et tests; conserver le contrat commercial une seule devise. |
| TR-08 | Deux catégories clients et profil fiscal eau daté; pas de reprise arbitraire TVA de l'électricité, ni modification rétroactive d'une facture émise. |

## C. Métier, historique, statut — P0/P1
| ID | Scénario |
|---|---|
| BI-01 | Nouveau client, consommations élec 100 kWh / eau 0 m³ ; exactement deux postes, même facture/numéro/total; aperçu, confirmation, détail, PDF, CSV/JSON. |
| BI-02 | Client issu base avant migration avec `water_enabled=0`; **deux postes sur nouvelle facture**, **statut réel de raccordement préservé ou `unknown`** et non requalifié en vrai sans preuve. |
| BI-03 | Eau positive avec barème source SEL confirmé, scénario de facturation au m³, TVA eau configurable et calcul exact. |
| BI-04 | Absence de volume eau, volume négatif ou invalide, > précision acceptée : bloquer; ne jamais assimiler vide à 0 sans consentement. |
| BI-05 | Eau 0 explicitement confirmée en prévisualisation; changement d'abonné/retour/reconnexion invalide l'ancienne confirmation; pas de facture zéro involontaire. |
| BI-06 | Ancien `document_template_version=1/2` avec seulement `ELECTRICITY`: titre/texte correspondant au document réel, **sans créer rétrospectivement de ligne d'eau**. |
| BI-07 | Facture 0 kWh / 0 m³ : deux services et total cohérent, absence de frais fictifs; taux valide connu ou placeholder honnête. |
| BI-08 | Paiement partiel, correction append-only, facture annulée, réédition PDF, suivi solde et états dashboard; aucun paiement effacé. |
| BI-09 | Prévisualisation puis changement de tarif, catégorie, client : préserver les informations figées en gardant un audit; ne pas confirmer un snapshot invalide/incohérent. |
| BI-10 | Une seule facture active par abonné/période; 2 POST concurrents, opérations transactionnelles; séquence et identifiant stable. |
| BI-11 | Migration SQLite répétée deux fois, avec anciens clients, factures/payments, et aucune altération des snapshots d'origine; triggers cohérents. |
| BI-12 | Sauvegarde cohérente `PRAGMA integrity_check`, restauration de copie de test, aucune base réelle modifiée; exports portable v1 compatibles et factures historiques intactes. |

## D. Windows et distribution — P0/P1
| ID | Scénario |
|---|---|
| W-01 | `python -m pytest -q`, tests complets, résultats exacts et tests nouveaux; pas de skip P0 silencieux. |
| W-02 | `python -m compileall -q` modules, `git diff --check`, packaging Windows `pyinstaller`, MSI WiX, bootstrapper Burn, contrôles version/assets. |
| W-03 | CI GitHub Actions sur **SHA final** de la PR et build green vérifiés; artefacts d'installation réels, taille+SHA-256 documentés. |
| W-04 | Windows standard-user; premier lancement, WebView2 présent/absent, GUI et PDF accessibles, pas de `requireAdministrator` ni bypass UAC. |
| W-05 | Upgrade depuis installateur publié 0.3.29 avec base représentative, sauvegarde avant update, clients/invoices/payments inchangés. |
| W-06 | Impression physique A4 : eau=0, eau>0, longue adresse, multipage, EUR non supporté si hors contrat, ancien document; aucun clipping. |
| W-07 | SafeUpdate avec clé publique de production encore absente : canal non opérationnel **fail-closed**; test de non-régression avec clé de test distincte, ne pas contourner Ed25519 pour livrer un EXE unsigned. |
| W-08 | Procédure recouvrement en cas de crash/mise à jour interrompue, sans restauration destructive; erreurs en langage clair. |

## E. Critères de sortie, preuve et interdictions
- **Toutes les anomalies P0 constatées doivent être corrigées ou marquées FAIL** et, dans ce cas, **NO-GO**.
- Un « PASS » prouve la commande, date, SHA, plateforme, code d'exit, résultat et référence de test/PDF; les tests auto seuls ne prouvent pas l'installation/impression sur matériel SEL.
- Les blocages externes `BLOCKED_EXTERNAL` (tarifs eau officiels, fiscalité, imprimante SEL, clé de prod SafeUpdate) sont **distincts** de bugs et la liste du besoin externe doit être remise au propriétaire.
- Aucun `skip` / `xfail` ciblant la fonction à valider, aucune fausse « signature Microsoft », aucun contournement de la sécurité Windows, aucune release/fusion non autorisée.
