"""Private, atomic SafeUpdate transaction records and final revalidation."""
from __future__ import annotations

import base64
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import tempfile
from uuid import uuid4

from safeupdate_crypto import (
    ManifestVerificationError,
    PackageVerificationError,
    parse_strict_json,
    verify_package,
    verify_signed_manifest,
)


TRANSACTION_ID = re.compile(r"\A[0-9a-f]{32}\Z")
TRANSACTION_SCHEMA = 1
MAX_TRANSACTION_BYTES = 256 * 1024


class TransactionVerificationError(ValueError):
    """Raised when local update state cannot be trusted."""


def default_staging_root() -> Path:
    import tempfile

    return Path(tempfile.gettempdir()) / "SEL-Facturation-Updater"


def state_path(*, root: Path | None = None) -> Path:
    if root is not None:
        return Path(root) / "update-state.json"
    try:
        import db

        return db.home() / "updates" / "update-state.json"
    except Exception:
        return Path.home() / "AppData" / "Local" / "SEL Facturation" / "updates" / "update-state.json"


def load_sequence_state(*, path: Path | None = None) -> dict:
    target = Path(path) if path is not None else state_path()
    try:
        value = json.loads(target.read_text(encoding="utf-8"))
    except (FileNotFoundError, OSError, UnicodeDecodeError, json.JSONDecodeError):
        return {"schema_version": 1, "sequence": 0, "manifest_sha256": ""}
    if not isinstance(value, dict) or value.get("schema_version") != 1:
        return {"schema_version": 1, "sequence": 0, "manifest_sha256": ""}
    sequence = value.get("sequence")
    digest = value.get("manifest_sha256")
    if not isinstance(sequence, int) or sequence < 0 or not isinstance(digest, str):
        return {"schema_version": 1, "sequence": 0, "manifest_sha256": ""}
    return {"schema_version": 1, "sequence": sequence, "manifest_sha256": digest}


def save_sequence_state(sequence: int, manifest_sha256: str, *, path: Path | None = None) -> Path:
    target = Path(path) if path is not None else state_path()
    target.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary_name = tempfile.mkstemp(prefix=f".{target.name}.", suffix=".tmp", dir=target.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump({"schema_version": 1, "sequence": sequence, "manifest_sha256": manifest_sha256}, stream, separators=(",", ":"))
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary_name, target)
    except Exception:
        Path(temporary_name).unlink(missing_ok=True)
        raise
    return target


def _root(root: Path | None) -> Path:
    return (Path(root) if root is not None else default_staging_root()).resolve()


def _transaction_directory(transaction_id: str, *, staging_root: Path | None = None) -> Path:
    if not isinstance(transaction_id, str) or not TRANSACTION_ID.fullmatch(transaction_id):
        raise TransactionVerificationError("invalid transaction id")
    root = _root(staging_root)
    transactions = (root / "transactions").resolve()
    candidate = (transactions / transaction_id).resolve()
    if not candidate.is_relative_to(transactions):
        raise TransactionVerificationError("transaction path escape")
    if candidate.exists() and candidate.is_symlink():
        raise TransactionVerificationError("transaction path escape")
    return candidate


def new_transaction_directory(*, staging_root: Path | None = None) -> tuple[str, Path]:
    root = _root(staging_root)
    transactions = root / "transactions"
    transactions.mkdir(parents=True, exist_ok=True)
    for _ in range(5):
        transaction_id = uuid4().hex
        candidate = transactions / transaction_id
        try:
            candidate.mkdir()
            return transaction_id, candidate
        except FileExistsError:
            continue
    raise TransactionVerificationError("unable to allocate transaction")


def _atomic_json(path: Path, payload: dict) -> None:
    fd, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump(payload, stream, ensure_ascii=False, separators=(",", ":"))
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary_name, path)
    except Exception:
        Path(temporary_name).unlink(missing_ok=True)
        raise


def write_transaction(transaction_id: str, *, staging_root: Path, package_name: str, update: dict) -> dict:
    directory = _transaction_directory(transaction_id, staging_root=staging_root)
    required = ("manifest_bytes", "signature_envelope_bytes", "manifest_sha256", "version", "sequence", "key_id", "sha256", "size_bytes")
    if any(key not in update for key in required):
        raise TransactionVerificationError("incomplete authenticated transaction")
    record = {
        "schema_version": TRANSACTION_SCHEMA,
        "transaction_id": transaction_id,
        "status": "ready",
        "package_name": package_name,
        "manifest_bytes": base64.b64encode(update["manifest_bytes"]).decode("ascii"),
        "signature_envelope_bytes": base64.b64encode(update["signature_envelope_bytes"]).decode("ascii"),
        "manifest_sha256": update["manifest_sha256"],
        "version": update["version"],
        "sequence": update["sequence"],
        "key_id": update["key_id"],
        "sha256": update["sha256"],
        "size_bytes": update["size_bytes"],
        "created_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    }
    _atomic_json(directory / "transaction.json", record)
    return record


def load_transaction(transaction_id: str, *, staging_root: Path | None = None) -> dict:
    directory = _transaction_directory(transaction_id, staging_root=staging_root)
    record_path = directory / "transaction.json"
    try:
        raw = record_path.read_bytes()
    except OSError as exc:
        raise TransactionVerificationError("transaction record unavailable") from exc
    try:
        record = parse_strict_json(raw, max_bytes=MAX_TRANSACTION_BYTES)
    except ManifestVerificationError as exc:
        raise TransactionVerificationError(str(exc)) from exc
    if not isinstance(record, dict) or record.get("schema_version") != TRANSACTION_SCHEMA or record.get("transaction_id") != transaction_id:
        raise TransactionVerificationError("invalid transaction record")
    return record


def verify_transaction(transaction_id: str, *, staging_root: Path | None = None, current_version: str, trusted_keys: dict[str, str | bytes]) -> dict:
    record = load_transaction(transaction_id, staging_root=staging_root)
    if record.get("status") not in {"ready", "launching"}:
        raise TransactionVerificationError("transaction is not ready")
    directory = _transaction_directory(transaction_id, staging_root=staging_root)
    package_name = record.get("package_name")
    if not isinstance(package_name, str) or Path(package_name).name != package_name or not package_name.lower().endswith(".exe"):
        raise TransactionVerificationError("package path escape")
    package = directory / package_name
    if package.is_symlink() or package.parent.resolve() != directory.resolve():
        raise TransactionVerificationError("package path escape")
    try:
        manifest_bytes = base64.b64decode(record["manifest_bytes"], validate=True)
        signature_bytes = base64.b64decode(record["signature_envelope_bytes"], validate=True)
    except (KeyError, ValueError, TypeError) as exc:
        raise TransactionVerificationError("invalid transaction evidence") from exc
    try:
        update = verify_signed_manifest(
            manifest_bytes, signature_bytes, current_version=current_version,
            trusted_keys=trusted_keys,
            now=datetime.now(timezone.utc),
        )
        if update["state"] != "available" or update["manifest_sha256"] != record.get("manifest_sha256"):
            raise TransactionVerificationError("transaction policy changed")
        verify_package(package, expected_sha256=record["sha256"], expected_size=record["size_bytes"])
    except (ManifestVerificationError, PackageVerificationError, KeyError, TypeError) as exc:
        raise TransactionVerificationError(str(exc)) from exc
    for key in ("version", "sequence", "key_id", "sha256", "size_bytes"):
        if update.get(key) != record.get(key):
            raise TransactionVerificationError("transaction evidence mismatch")
    return {**record, "package_path": package, "update": update}


def claim_transaction(transaction_id: str, *, staging_root: Path | None = None) -> dict:
    """Atomically claim a ready transaction for one installation attempt."""
    record = load_transaction(transaction_id, staging_root=staging_root)
    if record.get("status") != "ready":
        raise TransactionVerificationError("transaction already claimed")
    directory = _transaction_directory(transaction_id, staging_root=staging_root)
    lock = directory / ".launch.lock"
    try:
        fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        os.close(fd)
    except FileExistsError as exc:
        raise TransactionVerificationError("transaction already claimed") from exc
    try:
        return mark_transaction(transaction_id, "launching", staging_root=staging_root)
    except Exception:
        lock.unlink(missing_ok=True)
        raise


def release_transaction_lock(transaction_id: str, *, staging_root: Path | None = None) -> None:
    _transaction_directory(transaction_id, staging_root=staging_root).joinpath(".launch.lock").unlink(missing_ok=True)


def mark_transaction(transaction_id: str, status: str, *, staging_root: Path | None = None,
                     exit_code: int | None = None, backup_path: str | None = None) -> dict:
    record = load_transaction(transaction_id, staging_root=staging_root)
    if status not in {"ready", "launching", "installing", "completed", "failed", "recovery_required", "cancelled"}:
        raise ValueError("invalid transaction status")
    record["status"] = status
    if exit_code is not None:
        record["exit_code"] = int(exit_code)
    if backup_path is not None:
        record["backup_path"] = str(Path(backup_path).resolve())
    _atomic_json(_transaction_directory(transaction_id, staging_root=staging_root) / "transaction.json", record)
    return record
