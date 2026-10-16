"""Validation-only policy for signed SEL update packages.

Network download, signature verification, and installation stay outside the
request handler. This module only decides whether a manifest is safe to offer.
"""
from __future__ import annotations

import re
import hashlib
from pathlib import Path
from urllib.parse import urlparse


_SHA256 = re.compile(r"\A[0-9a-fA-F]{64}\Z")
_VERSION = re.compile(r"\A(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?\Z")


def _version(value: str) -> tuple[int, int, int, tuple[str, ...]]:
    match = _VERSION.fullmatch(str(value).strip())
    if not match:
        raise ValueError("invalid_version")
    pre = tuple(match.group(4).split(".")) if match.group(4) else ()
    return int(match.group(1)), int(match.group(2)), int(match.group(3)), pre


def _compare(left: str, right: str) -> int:
    a = _version(left)
    b = _version(right)
    if a[:3] != b[:3]:
        return (a[:3] > b[:3]) - (a[:3] < b[:3])
    if a[3] == b[3]:
        return 0
    if not a[3]:
        return 1
    if not b[3]:
        return -1
    for left, right in zip(a[3], b[3]):
        if left == right:
            continue
        left_num, right_num = left.isdigit(), right.isdigit()
        if left_num and right_num:
            return (int(left) > int(right)) - (int(left) < int(right))
        if left_num != right_num:
            return -1 if left_num else 1
        return (left > right) - (left < right)
    return (len(a[3]) > len(b[3])) - (len(a[3]) < len(b[3]))


def verify_sha256(path: str | Path, expected: str) -> bool:
    """Verify a downloaded package before it can be handed to an installer."""
    if not _SHA256.fullmatch(str(expected)):
        return False
    digest = hashlib.sha256()
    try:
        with Path(path).open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
    except OSError:
        return False
    return digest.hexdigest().lower() == expected.lower()


def evaluate_manifest(manifest: dict, *, current_version: str) -> dict:
    """Return ``available``, ``current`` or ``rejected`` without side effects."""
    try:
        version = str(manifest["version"])
        _version(version)
        _version(current_version)
    except (KeyError, TypeError, ValueError):
        return {"state": "rejected", "reason": "invalid_version"}
    url = manifest.get("url")
    if not isinstance(url, str) or urlparse(url).scheme != "https" or not urlparse(url).netloc:
        return {"state": "rejected", "reason": "https_required"}
    if not isinstance(manifest.get("sha256"), str) or not _SHA256.fullmatch(manifest["sha256"]):
        return {"state": "rejected", "reason": "invalid_sha256"}
    if manifest.get("package_type") != "bootstrapper":
        return {"state": "rejected", "reason": "unsupported_package"}
    if manifest.get("product") != "SEL-Facturation":
        return {"state": "rejected", "reason": "unsupported_product"}
    comparison = _compare(version, current_version)
    if comparison < 0:
        return {"state": "rejected", "reason": "downgrade_blocked"}
    if comparison == 0:
        return {"state": "current", "reason": "already_current", "version": version}
    return {"state": "available", "version": version, "url": url,
            "sha256": manifest["sha256"].lower(), "product": manifest["product"]}
