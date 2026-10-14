# Bootstrapper Windows

`Bundle.wxs` enchaîne l'installation autonome WebView2 puis le MSI SEL. Le
package WebView2 doit être récupéré depuis la distribution officielle
Microsoft pendant le build et conservé avec les artefacts de la release.

Le bootstrapper n'efface pas les données métier et doit être testé sur une
VM Windows 10 et une VM Windows 11 propres avant toute livraison SEL.
