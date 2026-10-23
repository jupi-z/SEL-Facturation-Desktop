"""Generate an Ed25519 SafeUpdate key pair outside the repository."""
from __future__ import annotations

import argparse
import base64
import getpass
from hashlib import sha256
from pathlib import Path
import sys

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--private-key", type=Path, required=True)
    parser.add_argument("--no-password", action="store_true", help="Tests uniquement; interdit pour la production.")
    args = parser.parse_args()
    destination = args.private_key.resolve()
    try:
        destination.relative_to(ROOT.resolve())
    except ValueError:
        pass
    else:
        raise ValueError("La clé privée doit être enregistrée hors du dépôt.")
    key = Ed25519PrivateKey.generate()
    password = None
    if not args.no_password:
        first = getpass.getpass("Nouvelle phrase secrète : ")
        second = getpass.getpass("Répétez la phrase secrète : ")
        if not first or first != second:
            raise ValueError("Phrase secrète vide ou différente.")
        password = first.encode()
    encryption = serialization.NoEncryption() if password is None else serialization.BestAvailableEncryption(password)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, encryption))
    raw_public = key.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
    print(f"public_key_base64={base64.b64encode(raw_public).decode('ascii')}")
    print(f"public_key_sha256={sha256(raw_public).hexdigest()}")
    print("Conservez la clé privée hors du dépôt et sauvegardez-la chiffrée.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"SafeUpdate key generation refused: {exc}", file=sys.stderr)
        raise SystemExit(1)
