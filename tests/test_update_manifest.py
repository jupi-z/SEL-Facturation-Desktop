import json
from pathlib import Path

from update_policy import evaluate_manifest


ROOT = Path(__file__).resolve().parents[1]


def test_published_update_manifest_stays_on_the_last_published_preview_release():
    manifest = json.loads((ROOT / "updates" / "latest.json").read_text(encoding="utf-8"))

    # 0.3.25 is being validated locally and is not published yet. The public
    # manifest must not advertise a URL that does not exist on GitHub.
    assert manifest["version"] == "0.3.24"
    assert manifest["channel"] == "preview"
    assert manifest["signed"] is False
    assert evaluate_manifest(manifest, current_version="0.3.22")["state"] == "available"
