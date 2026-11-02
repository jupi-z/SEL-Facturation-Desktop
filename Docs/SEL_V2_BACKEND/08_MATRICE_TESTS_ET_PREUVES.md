# Matrice de recette impérative — SEL V2 Backend

Chaque scénario est `PASS`, `FAIL`, `BLOCKED_EXTERNAL` ou `NOT_TESTED`; chaque PASS contient nom du test, commande, environnement, SHA, résultat exact, entrée synthétique et lien preuve/CI. **Aucune donnée SEL réelle** dans tests ni rapports.

## API indépendante
| ID | Test |
|---|---|
| API-01 | `sel-api` démarre avec PostgreSQL et renvoie health sans interface, Python ni pywebview |
| API-02 | OpenAPI 3.1 valide et routes de lecture/écriture conformes aux DTO, schema evolution |
| API-03 | Auth + RBAC sur chaque endpoint, 401/403, token expiré, IDOR organisation/client |
| API-04 | Création client puis lecture/pagination/filtre + contrôle ETag/version |
| API-05 | 50 POST identiques avec même Idempotency-Key => 1 effet; même key payload différent => 409 |
| API-06 | Aucun endpoint ou Content-Type `application/pdf`, pas de file writing d'une facture côté server |

## Moteur commercial
| ID | Test |
|---|---|
| FIN-01 | 100 kWh @0,095 USD + TVA électrique 16% + 0 m³ = 11,02 USD avec deux postes obligatoires; **fixture uniquement** |
| FIN-02 | Eau 0 sans tarif eau connu : `0 m³` et 0,00, pas de faux tarif ni exonération; eau>0 sans tarif/profil approuvé => 422 |
| FIN-03 | Eau>0 avec tarifs/profil fiscal **test seulement** : sous-totaux et total, devise identique, snapshot figé |
| FIN-04 | Facture ordinaire 11,02 USD + paiement 3 USD => solde 8,02 USD, **aucun installment_plan** |
| FIN-05 | Solar sale 100 USD, schedule 50+50 => **une seule receivable de 100 USD**; payer 20 puis 30 => solde 50 USD |
| FIN-06 | Échéance 50 payée 20 → retard selon date/exigibilité; paiement anticipé et affectation correcte; somme échéances = principal |
| FIN-07 | Paiements concurrents, allocation excédentaire et reverse append-only; aucune double écriture |
| FIN-08 | Avoir/annulation, compensation et taxe par service; anciennes factures inchangées |
| FIN-09 | Currency EUR invalide si pas configuré, USD/CDF taux source/arrondi exact, aucun float |
| FIN-10 | Tarifs inactifs/non confirmés/supplantés, expiration grant offline => refus |
| FIN-11 | Numéro provisoire local vs numéro final central unique; ne pas générer fausse facture fiscale offline |
| FIN-12 | Requêtes non valides n'occupent pas numéro unique final ni ne laissent une transaction financière partielle |

## Sync offline et durabilité
| ID | Test |
|---|---|
| SYN-01 | Réseau indisponible dès démarrage, agent local démarre, lit cache, écrit opérations couvertes par offline grant |
| SYN-02 | Coupure pendant 100 transactions, chaque commande et outbox atomiques (SQLite réel), redémarrage sans perte |
| SYN-03 | Push puis coupure juste après COMMIT central et avant ACK → retry, 1 seul paiement/facture |
| SYN-04 | Pull 3 pages, crash avant cursor commit → application idempotente et cursor correct |
| SYN-05 | Deux appareils même client/période : une facture centrale, un conflit visible, documents locaux non effacés |
| SYN-06 | Deux paiements offline sur même solde : aucun événement perdu; dépassement ne s'impute pas silencieusement |
| SYN-07 | Echéancier obsolète: conflit de révision; pas de changement silencieux des dates |
| SYN-08 | Device ou utilisateur révoqué : rejet central; local refuse nouveaux événements privilégiés si grant expiré |
| SYN-09 | Sync reconnect, outbox drain, pull convergent, hash & counts, statut `CENTRAL_ACCEPTED` |
| SYN-10 | Reset/rebootstrap local de projection ne supprime pas opérations locales pending |
| SYN-11 | Statuts client/local/central exposés sans ambiguïté dans JSON; aucun « synchronisé » avant ACK |
| SYN-12 | Multi device avec achat kit en stock limité : réservation ou vente pendante, pas de stock fictif garanti |

## LiveUpdate / SafeUpdate
| ID | Test |
|---|---|
| UPD-01 | Bundle déclaratif signé valide active nouvelle config sans remplacer EXE |
| UPD-02 | Signature/hash/expiration/version incorrecte => refus, ressources antérieures fonctionnelles |
| UPD-03 | Activation ratée → rollback local sûr, sans modifier factures |
| UPD-04 | Version Rust native incompatible => demande SafeUpdate; pas d'interprétation arbitraire de code à privilège |
| UPD-05 | Clé publique production non installée => canal production fail-closed, fixture de test distincte |

## Migration, fichiers facture et exploitation
| ID | Test |
|---|---|
| MIG-01 | Ancienne SQLite 0.3.x anonymisée, import de factures/paiements/annulations, snapshots identiques |
| MIG-02 | Dry run ne modifie rien, import répété trois fois sans doublon |
| MIG-03 | Rapports de comparaison par devise, facture, client; solde égal, aucune créance dupliquée |
| MIG-04 | Erreur migration à mi-parcours : rollback/reprise et sources intactes |
| MIG-05 | `pg_dump` + restauration sur DB de test, totals et IDs vérifiés |
| FILE-01 | Requête de facture via API = `application/json` et aucune réponse `application/pdf`; scan routes |
| FILE-02 | Inspecter schéma PG: aucune colonne/relations `invoice_pdf`, blob `bytea`, chemin/URL fichier de facture |
| FILE-03 | Après création et récupération de 100 factures, inspecter filesystem serveur, volumes, logs, caches, exports/backups : **aucun fichier PDF facture client** |
| FILE-04 | Générer PDF depuis `document-data` sur poste local/test CLI, avec snapshot et template versionné; accès serveur non requis pour facture déjà cachée |
| OPS-01 | `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace` verts; intégration PG/SQLite avec migrations |
| OPS-02 | `docker compose` local (API + PostgreSQL) démarrable, health/check migration, DB non exposée |
| OPS-03 | Tests TLS, RBAC, logs sans PII/secret, backup chiffrée, accès minimes |
| OPS-04 | Tests sur vrai PC Windows SEL + vrai VPS + impression et tarification SEL : **NOT_TESTED** sans matériel/accès/décision SEL |

## Gate de fin
- `GO_REVIEW` si aucun P0 logiciel/financier/security FAIL et preuve API+PG+SQLite; `GO_PILOT` seulement si tests offline, sync et restauration réels validés en environnement représentatif.
- `GO_PRODUCTION` impossible sans validation finale du propriétaire, serveur, sécurité, droit d'émission et prix/taxes SEL. **Ne pas fusionner ni déployer implicitement**.
- Dans le rapport `IMPLEMENTATION_REPORT.md`, lister **tous les identifiants** de ce document, pourcentage = nb PASS / total, et indiquer toute limite externe sans la transformer en PASS.
