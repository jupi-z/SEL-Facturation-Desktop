from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def _caddy_site_block(caddyfile: str, hostname: str) -> str:
    lines = caddyfile.splitlines()
    start = next(index for index, line in enumerate(lines) if line.strip() == f"{hostname} {{")
    depth = 0
    block = []
    for line in lines[start:]:
        depth += line.count("{") - line.count("}")
        block.append(line)
        if depth == 0:
            return "\n".join(block)
    raise AssertionError(f"Unclosed Caddy site block for {hostname}")


def test_production_api_domain_only_routes_to_sel_api():
    caddyfile = (ROOT / "v2" / "deploy" / "Caddyfile.example").read_text(encoding="utf-8")
    api_site = _caddy_site_block(caddyfile, "sel.shuleops.com")

    assert "reverse_proxy sel-api:8080" in api_site
    assert "file_server" not in api_site


def test_update_domain_serves_only_gettable_update_assets_from_read_only_mount():
    caddyfile = (ROOT / "v2" / "deploy" / "Caddyfile.example").read_text(encoding="utf-8")
    compose = (ROOT / "v2" / "deploy" / "compose.production.example.yml").read_text(encoding="utf-8")
    update_site = _caddy_site_block(caddyfile, "update.sel.shuleops.com")

    assert "/updates/*" in update_site
    assert "/releases/*" in update_site
    assert "method GET HEAD" in update_site
    assert "root * /srv/sel-updates" in update_site
    assert "file_server" in update_site
    assert "respond 404" in update_site
    assert "respond 405" in update_site
    assert "./updates:/srv/sel-updates:ro" in compose
