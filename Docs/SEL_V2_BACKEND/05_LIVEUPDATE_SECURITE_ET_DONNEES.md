# SEL LiveUpdate + sécurité opérationnelle — ressources sans nouvel exécutable

## Deux canaux à ne jamais confondre
**LiveUpdate** ne remplace aucun EXE Windows. Il diffuse des éléments **déclaratifs** : tarifs et taux SEL après approbation, types de client, règles configurables bornées, textes, versions des schémas JSON, templates documentaires locaux, et **ultérieurement** assets HTML/CSS/JS de l'UI lorsqu'une sandbox sera prête. Les évolutions du serveur Rust peuvent être déployées côté serveur **sans toucher au client** uniquement si le protocole et ses capacités locales restent compatibles.

**SafeUpdate** remplace le composant natif/agent Rust ou son installateur lorsqu'une fonctionnalité nécessite un nouvel EXE / une migration non compatible. Le programme Windows peut rester sans signature Authenticode en phase pilote, mais le manifest Ed25519 authentifié + SHA256 exact reste impératif, et Windows peut bloquer un paquet. Ne jamais supprimer UAC/SmartScreen.

## Protocole LiveUpdate
1. Canal `stable/preview` explicitement choisi, cible `device_class`, version minimale et maximale compatible du moteur local, version de schéma et `sequence` anti rollback. Source de confiance = clé publique Ed25519 **épinglée dans une version initiale installée de confiance**.
2. Manifeste signé sur octets exacts, `expires_at`, tailles/hash SHA256 pour chaque bundle, type autorisé (`configuration`, `invoice_template`, `ui_assets` futur), hash de chaque bundle. Refuser inconnu, dépassé, mélange manifeste/assets, downgrade, signature invalide, chemin/symlink d'évasion, cross-origin non approuvé.
3. Télécharger dans staging privé de l'utilisateur, vérifier taille/hash/signature, compatibilité et **ne jamais exécuter du code non approuvé**. Séparer templates déclaratifs de scripts; les schémas de templates sont bornés et non Turing-complets.
4. Valider en parallèle avec état courant sur données synthétiques, sauvegarder dernière version `known-good`, activer par bascule **atomique**; si mauvais chargement/crash, revenir au bundle précédent. Ne jamais modifier un snapshot financier émis par une mise à jour de template.
5. Un template versionné ne doit contenir aucune donnée réelle de facture; l'application combine localement `document-data` + template; pas de PDF généré/stocké sur le serveur.
6. En offline utiliser le dernier bundle de confiance déjà installé, même si réseau absent; une expiration de métadonnées interdit **l'installation de nouvelles ressources non vérifiées**, pas l'accès aux factures en cache déjà vérifiées.
7. Audit `device_id, bundle_id, version, installed_at, previous_version, verification_result` sans nom de client, facture PDF ni secrets.

## Sécurité API et identité
- HTTPS public; serveur Rust derrière reverse proxy avec TLS, limites taille requêtes/temps, logs structurés sans PII et sans montant en clair quand inutile; PostgreSQL réseau privé, compte DB moindre privilège et rotation secrets.
- RBAC au minimum : `ADMIN`, `BILLING_AGENT`, `CASHIER`, `SUPERVISOR`, `AUDITOR_READONLY`. Contrôle objet et organisation **à chaque endpoint**, pas uniquement au frontend. Aucune élévation de rôle par champ JSON.
- Appareils enrôlés et limités. Rotation sessions, verrouillage local, journal tentatives, throttling, capacité offline limitée, procédure révocation; tests IDOR et accès à des comptes clients hors périmètre.
- Idempotency Key et validation de payload; guard against replay; données monétaires jamais `float`; journaux financiers append-only.
- Sauvegardes PostgreSQL chiffrées, restaurations testées, sauvegardes SQLite appropriées, secrets hors GitHub; documents officiels et QRs DGI **non simulés comme homologués**.
- Confidentialité : aucune donnée client réelle dans tests, crash reports, logs d'update, artifacts CI; **aucun PDF reçu ou conservé par l'API**.
- Mises à jour de sécurité natives restent possibles/nécessaires; ne pas promettre que les mises à jour internes suppriment tous les nouveaux installateurs.

## Tests
UPD-01 signature valide active config sans EXE; UPD-02 signature fausse/hash modifié → refus; UPD-03 manifest expiré / rollback / key_id inconnu → refus; UPD-04 fichier template hors dossier staging/zip traversal → refus; UPD-05 activation ratée → restaure known-good; UPD-06 offline conserve ressources trusted; UPD-07 moteur Rust incompatible → demande SafeUpdate, ne force pas un nouveau bundle; UPD-08 pas de clé prod → release prod **NO-GO**, jamais signature test en prod; SEC-01 401/403/IDOR tests; SEC-02 aucune trace PDF/fichier facture sur serveur après test complet; SEC-03 logs expurgés.
 
## Références
- The Update Framework : https://theupdateframework.io/docs/metadata/
- OWASP API : https://api-security.owasp.org/editions/2023/fr/0x11-t10/
- SQLite WAL : https://www.sqlite.org/wal.html
- Tauri (futur client seulement), permissions : https://tauri.app/security/capabilities/
