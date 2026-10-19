import json
from pathlib import Path

from version import APP_VERSION
from update_policy import evaluate_manifest


ROOT = Path(__file__).resolve().parents[1]


def test_published_update_manifest_matches_the_current_preview_release():
    manifest = json.loads((ROOT / "updates" / "latest.json").read_text(encoding="utf-8"))

    assert manifest["version"] == APP_VERSION
    assert manifest["channel"] == "preview"
    assert manifest["signed"] is False
    assert evaluate_manifest(manifest, current_version="0.3.22")["state"] == "available"
