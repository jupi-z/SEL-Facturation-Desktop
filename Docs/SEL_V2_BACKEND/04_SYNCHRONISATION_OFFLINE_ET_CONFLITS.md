# Contrat SEL Sync V1 — offline-first, reprise fiable et conflits financiers

## Objectif
Permettre à l'agent Windows de **travailler localement sans Internet** (consultations autorisées, création client, préparation factures eau/électricité, encaissements et ventes solaires) pendant une période de droits offline valide; et synchroniser correctement lorsque la connexion revient. Une donnée locale est **EN ATTENTE** tant qu'aucun commit PostgreSQL ne l'a reconnue. Ne pas promettre « exactement une fois » sur le transport réseau.

## Enveloppe immuable de commande
```json
{
 "protocol_version":1, "event_id":"<UUID>", "device_id":"<UUID>",
 "organization_id":"<ORG_CONTEXT>", "actor_id":"<UUID>",
 "command_type":"PAYMENT_RECORDED", "aggregate_id":"<UUID>",
 "aggregate_expected_version":4, "occurred_at":"<UTC_ISO8601>",
 "offline_grant_id":"<UUID>", "payload":{ "payment_id":"<UUID>","amount":"15.00","currency":"USD" },
 "payload_sha256":"<HASH_TEST>", "client_schema_version":1
}
```
Chaque événement est persisté **avec** l'opération métier et le journal append-only dans **une transaction SQLite**. Les IDs sont générés par le client pour pouvoir les répéter après panne. `event_id` unique et `payload_sha256` invariant. Authentifier le device/compte : session + clé de l'appareil et autorisation serveur; une signature locale ne remplace pas RBAC.

## Protocole
1. `POST /v1/sync/push` : batch ordonné et borné, limites bytes/temps, `device_id`, `events[]`; serveur valide la version, scopes, grants, horloge/périodes, état du client/contrat, montant, tarif/snapshot, révision et clé d'idempotence.
2. En **une transaction PostgreSQL par unité d'acceptation**, créer l'empreinte inbox unique `(device_id,event_id)`, appliquer la commande, inscrire les changements de projection et écrire réponse durable. En retry identique renvoyer **exactement le résultat initial**; même ID, payload différent → conflit de sécurité.
3. Réponses par événement : `ACCEPTED{server_sequence,server_entity_id,number?}`, `DUPLICATE{same_original_result}`, `CONFLICT{reason,central_revision}`, `REJECTED{code,required_action}`. **Pas** de `ACCEPTED` après simple réception HTTP sans commit.
4. Le client marque ack uniquement après persistence locale de la réponse; ne supprime pas l'opération ni son journal tant que sécurité/rétention ne le permet. Coupure réseau après commit PG avant ack = retry dédupliqué.
5. `GET /v1/sync/pull?cursor=...` : modifications consolidées autorisées et paginées, `next_cursor` monotone/opaque, tombstones et revisions pour supprimer/archiver logiquement. Curseur persisté **dans même transaction SQLite** que l'application des changements reçus. Cursor invalidé → bootstrap de projection contrôlé, **sans effacer outbox locale**.
6. `/sync/status` : heure sync dernière réussie, pending, conflits, rejected, version de protocole, état des droits offline, sans fuite de données clients dans logs.

## Politique de conflits (PAS de last-write-wins financier)
| Situation | Réaction sûre |
|---|---|
| Modification coordonnées client sur 2 appareils | Conflit de révision, comparaison, résolution responsable ou fusion de champs explicitement non financiers |
| Deux créations factures même client/période | Unique constraint central; deuxième **CONFLICT**, jamais double émission finale |
| Deux paiements offline sur même créance | **Conserver les deux événements**; serveur arbitre selon solde restant, n'encaisse jamais doublement sans alerte. Si dépassement, `CONFLICT_REQUIRES_REVIEW` et procédure d'excédent/remboursement; ne pas perdre preuve de réception physique de cash |
| Paiement enregistré puis internet coupé avant ACK | retry idempotent, une écriture métier unique |
| Échéancier changé pendant coupure | Contrôle `expected_version`; ne pas écraser les dates/conditions du contrat sans consentement |
| Tarif modifié central pendant offline | Utiliser snapshot/prix signé de référence et politique de validité du grant. Incompatibilité → demande de révision, ne pas recalculer silencieusement une facture déjà imprimée |
| Vente simultanée dernier kit physique | Sans mécanisme de réservation/pré-allocation stock, **bloquer vente offline garantie stock**, ou marquer commande en attente sans promettre disponibilité |
| Device perdu/révoqué offline | Expiration du grant local + révocation appliquée au prochain contact; **pas de révocation instantanée sans réseau** |

## Identification et numérotation
- UUID généré localement stable pour clients, paiements, invoices et transactions.
- Pour documents offline, imprimer une **référence locale provisoire distincte** (ou pré-réservation de série officielle validée); ne pas inventer que la numérotation officielle est acquise tant que central ne l'a pas confirmée.
- Ne jamais changer silencieusement un document remis comme « définitif » après synchronisation. Garder mapping `provisional_ref → invoice_number` et preuve d'état; régularisation selon procédure SEL/juridique validée.

## Gestion des droits et durabilité
- Auth initiale en ligne + offline grant signé limité (rôles, appareils, catégories, montants/encaissements, TTL et limites de cumul). En absence/expiration : consultation de cache et brouillons autorisés selon politique; **pas** d'encaissements/émissions privilégiés sans autorisation valide.
- Compter les plafonds offline par appareil et cumul local pour limiter doubles ventes/encaissements; au retour central vérifier contre règles globales.
- SQLite avec journal WAL local, checkpoints, transaction atomique; chiffrer au repos les données privées avec clé protégée Windows et sauvegarde recouvrable **avant pilote réel**. Ne pas afficher la clé dans logs.
- Plusieurs postes n'ont pas une vérité commune avant reconnexion; affichage honnête de l'état « local / en attente / accepté / conflit » obligatoire même dans tests CLI.

## Tests d'acceptation
SYN-01 : écrire 100 commandes réseau coupé, redémarrer local, outbox intacte, toutes opérations licites; SYN-02 : push retries 10 fois, une seule facture/paiement; SYN-03 : couper juste après PG commit avant ACK, reprise sans doublon; SYN-04 : pull pages, crash entre DB apply/cursor commit, reprendre sans pertes; SYN-05 : 2 appareils créent même facture/période, un seul numéro final; SYN-06 : 2 encaissements sur même dette, pas de perte et conflit explicite; SYN-07 : retard / expiration grant, permissions appliquées; SYN-08 : migration DB SQLite ne perd pas les pending; SYN-09 : ancien tarif conflit, état en attente, pas réécriture historique; SYN-10 : device compromis/révoqué, rejet central.

## Contraintes de temps
Ne pas implémenter des algorithmes génériques CRDT, multi-master replication ni réplication PostgreSQL directe en 23 h; cibler commandes métier V1, testables et conservatrices. Couvrir **une** flotte de 2 postes de test avec conflits; toute fonctionnalité non vérifiée demeure `NO-GO` pour exploitation.
