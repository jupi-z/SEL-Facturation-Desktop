# SEL SafeUpdate V1 — dossier normatif Codex

**Préparé le 22 octobre 2026.** Le dossier contient seulement des spécifications et une mission. Il **ne change pas** encore le logiciel, ses binaires, le manifeste public, ses données ou son installateur.

**Entrée de Codex :** [CODEX_SAFEUPDATE_START_HERE.md](../../CODEX_SAFEUPDATE_START_HERE.md)

1. [00 — Mission Codex](00_MISSION_CODEX.md)
2. [01 — Spécification normative](01_SPECIFICATION_NORMATIVE.md)
3. [02 — Protocole cryptographique et menaces](02_PROTOCOLE_CRYPTO_ET_MENACES.md)
4. [03 — Release, packaging et installation](03_RELEASE_PIPELINE_ET_INSTALLATION.md)
5. [04 — Tests et acceptation](04_MATRICE_TESTS_ET_ACCEPTATION.md)
6. [05 — Plan d'exécution](05_PLAN_EXECUTION_ET_DEPLOIEMENT.md)
7. [06 — Exploitation des clés et déploiement](06_OPERATIONS_CLES_ET_ROLLOUT.md)

## Différence impérative
**Non signé Authenticode** ≠ **non authentifié**. Les exécutables Windows peuvent rester non signés; **le manifeste doit être signé Ed25519**, et le binaire téléchargé doit correspondre au SHA256 signé.

**Ne pas contourner Windows.** Installer en contexte utilisateur quand possible et gérer honnêtement les cas UAC / SmartScreen / contrôle d'application.

## Sources publiques de référence
- [Windows Installer et UAC](https://learn.microsoft.com/en-us/windows/win32/msi/using-windows-installer-with-uac)
- [Windows SmartScreen reputation](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)
- [Distribution WebView2](https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution)
- [Ed25519 dans cryptography](https://cryptography.io/en/latest/hazmat/primitives/asymmetric/ed25519/)

Un projet qui prétend « Windows n'interviendra jamais » ou « tout fonctionne sans clef de confiance » n'est pas conforme à ce dossier.
