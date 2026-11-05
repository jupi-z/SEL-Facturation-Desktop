# SEL update distribution root

This directory is mounted read-only by Caddy at `/srv/sel-updates` and is served only from `update.sel.shuleops.com` for GET/HEAD requests under `/updates/`, `/releases/`, and `/resources/`.

Keep it empty until a production signing key, verified release artifacts, and an atomic publishing procedure are approved. Never place private keys, local databases, customer data, invoice PDFs, or logs here. The configured `latest.json` and `latest.sig.json` paths intentionally return 404 until a signed channel is published.
