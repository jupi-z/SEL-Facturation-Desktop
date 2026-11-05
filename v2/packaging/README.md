# Ancien packaging V2 local — retiré

Ce packaging PyWebView + `sel-local` est obsolète et son script de build est
volontairement bloqué. Il ne doit plus servir à produire ou distribuer SEL.

Le client Windows actuel est `v2/windows-app` (Tauri). Il exige une session
authentifiée et communique avec le serveur SEL pour chaque opération métier.
Il n’embarque ni agent `sel-local`, ni base métier SQLite, ni outbox hors ligne.

Les anciennes sources, migrations et contrats de synchronisation sont conservés
pour la traçabilité et les migrations contrôlées. Ce maintien ne signifie pas que
le client Windows offre un mode local.
