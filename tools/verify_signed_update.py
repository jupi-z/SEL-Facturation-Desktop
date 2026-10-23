"""Offline SafeUpdate V1 publication verifier; uses only public key material."""
from __future__ import annotations

import argparse
import base64
from datetime import datetime, timezone
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from safeupdate_crypto import verify_signed_manifest, verify_package


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--signature", type=Path, required=True)
    p.add_argument("--package", type=Path, required=True)
    p.add_argument("--key-id", required=True)
    p.add_argument("--public-key-base64", required=True)
    p.add_argument("--current-version", required=True)
    p.add_argument("--channel", choices=("stable", "preview"), default="stable")
    args = p.parse_args()
    manifest_bytes = args.manifest.read_bytes()
    signature_bytes = args.signature.read_bytes()
    update = verify_signed_manifest(
        manifest_bytes, signature_bytes, current_version=args.current_version,
        trusted_keys={args.key_id: args.public_key_base64}, allowed_channel=args.channel,
        now=datetime.now(timezone.utc),
    )
    if update["state"] not in {"available", "current"}:
        raise ValueError("manifest is not a valid update")
    verify_package(args.package, expected_sha256=update["sha256"], expected_size=update["size_bytes"])
    print(json.dumps({key: value for key, value in update.items() if key not in {"manifest", "manifest_bytes", "signature_envelope_bytes"}}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"SafeUpdate verification failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
