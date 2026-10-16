"""Validation-only policy for signed SEL update packages.

Network download, signature verification, and installation stay outside the
request handler. This module only decides whether a manifest is safe to offer.
"""
from __future__ import annotations

import re
from urllib.parse import urlparse


_SHA256 = re.compile(r"\A[0-9a-fA-F]{64}\Z")
_VERSION = re.compile(r"\A(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:[-+]([0-9A-Za-z.-]+))?\Z")


def _version(value: str) -> tuple[int, int, int, str]:
    match = _VERSION.fullmatch(str(value).strip())
    if not match:
        raise ValueError("invalid_version")
    pre = match.group(4) or ""
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
    return (a[3] > b[3]) - (a[3] < b[3])


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
    comparison = _compare(version, current_version)
    if comparison < 0:
        return {"state": "rejected", "reason": "downgrade_blocked"}
    if comparison == 0:
        return {"state": "current", "reason": "already_current", "version": version}
    return {"state": "available", "version": version, "url": url}
