# Dossier de passation Codex — SEL facture eau + électricité

**Intégré au dépôt en documentation uniquement le 22 octobre 2026. Le code métier et l'installateur ne sont pas modifiés par cette passation.**

Contenu, dans l'ordre de lecture :

1. `00_MISSION_CODEX.md` — mandat prêt à copier/transmettre.
2. `01_SPECIFICATION_NORMATIVE.md` — cahier des charges métier, technique, fiscal et données.
3. `02_TESTS_ET_RECETTE.md` — tests, cas chiffrés, QR, PDF, compatibilité et seuil GO/NO-GO.
4. `03_DECISIONS_A_CONFIRMER_SEL.md` — informations qui ne doivent **pas** être devinées.
5. `04_MAQUETTE_A4.md` — composition PDF A4 cible sous forme de wireframe texte.

**Source du projet :** https://github.com/jupi-z/SEL-Facturation-Desktop  
**Commit de référence observé en lecture seule :** `baf7a88aae9696b29dff4fd45919daad861c9d7a`.

**Décision métier acquise :** SEL souhaite une facture unique eau + électricité. Cependant, la tarification et la fiscalité de l'eau ne sont pas établies dans les éléments à notre disposition. Le dossier impose un blocage contrôlé de l'émission combinée réelle tant que ces éléments ne sont pas validés. Il autorise des scénarios fictifs de test sans contaminer la base réelle.

**Documents source :** ce sous-dossier contient la mission, les spécifications, la recette, les décisions à confirmer et la maquette A4. Le fichier racine `CODEX_START_HERE.md` est le point d'entrée direct.\n\n**Restrictions :** pas de refonte de l'application, pas d'intégration Monarque Connect ni DGI à ce stade ; historique et export portable conservés ; QR de référence interne ≠ QR fiscal. Codex peut préparer une PR de revue seulement lorsque le donneur d'ordre lance explicitement son exécution. Ne jamais fusionner/publier sans son accord.