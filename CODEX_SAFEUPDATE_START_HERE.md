# Codex — SEL SafeUpdate V1 : mises à jour authentifiées sans Authenticode

> **Nouvelle mission distincte** de la facture eau + électricité. Point d'entrée uniquement pour le chantier SafeUpdate. Publication documentaire du 22 octobre 2026. **Aucun code n'a été modifié pour SafeUpdate à ce stade.**

## Démarrage requis
Lire **intégralement, dans l'ordre** :
1. [Mission](Docs/SEL_SAFEUPDATE_V1/00_MISSION_CODEX.md)
2. [Spécification normative](Docs/SEL_SAFEUPDATE_V1/01_SPECIFICATION_NORMATIVE.md)
3. [Protocole cryptographique Ed25519 et menaces](Docs/SEL_SAFEUPDATE_V1/02_PROTOCOLE_CRYPTO_ET_MENACES.md)
4. [Release CI et installation](Docs/SEL_SAFEUPDATE_V1/03_RELEASE_PIPELINE_ET_INSTALLATION.md)
5. [Matrice de tests et acceptation](Docs/SEL_SAFEUPDATE_V1/04_MATRICE_TESTS_ET_ACCEPTATION.md)
6. [Plan d'exécution](Docs/SEL_SAFEUPDATE_V1/05_PLAN_EXECUTION_ET_DEPLOIEMENT.md)
7. [Exploitation des clés et déploiement](Docs/SEL_SAFEUPDATE_V1/06_OPERATIONS_CLES_ET_ROLLOUT.md)
8. [Index du dossier](Docs/SEL_SAFEUPDATE_V1/README.md) puis [AGENTS.md](AGENTS.md).

## Mandat confirmé du propriétaire
Concevoir et **implémenter sur branche et PR distinctes** un mécanisme de mise à jour automatique en un clic pour SEL Facturation qui accepte les **paquets Windows non signés Authenticode**, MAIS seulement avec un **manifeste signé Ed25519 par SEL et une vérification stricte du hash SHA256 du paquet**.

Aucune désactivation / bypass des protections Windows. L'application reste per-user, avec SQLite protégée et sauvegardes, et n'exige pas une ouverture en administrateur. Les comportements SmartScreen/UAC dépendant de la machine ne sont pas garantis ; les blocages doivent être honnêtement rapportés.

**Prudence :** la version existante 0.3.24 refuse les mises à jour non signées Authenticode; une **première installation de transition** sera nécessaire pour établir la nouvelle clé publique de confiance, avant que les upgrades futurs deviennent autonomes.

## Limites d'autorisation
Codex peut analyser, coder, tester, construire localement, produire des preuves et ouvrir sa PR. **Ne pas fusionner main, publier une release ou installer sur un vrai PC SEL sans nouvelle autorisation du propriétaire.** L'actuelle PR #41 concerne la facture eau + électricité, et ne doit pas être altérée par SafeUpdate.

Ne jamais fournir en réponse une clé privée de production; la générer sur un poste détenu par le propriétaire, hors dépôt et avec procédure claire. Si la clé n'est pas disponible, faire un E2E avec des clés de test, **ne pas** prétendre à une release de production.

## Rapport final
Produire `Docs/SEL_SAFEUPDATE_V1/IMPLEMENTATION_REPORT.md` sur la PR avec version / commit / tests / preuves / limites / GO-NO-GO, incluant le nombre exact de tests et leur résultat. La mission n'est achevée que lorsque la chaîne update cryptographiquement authentifiée et les vérifications d'installation sont réellement testées.
