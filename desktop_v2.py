"""Native Windows shell for the SEL V2 local Rust agent.

The shell is deliberately thin: it owns the native window and the local
process lifecycle, while all commercial rules and persistence remain in Rust.
The browser surface never receives the local bearer token; requests cross the
pywebview bridge and are authenticated by this process supervisor.
"""

from __future__ import annotations

import json
import logging
import os
from pathlib import Path
import secrets
import subprocess
import sys
import time
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen
from uuid import uuid4


APP_TITLE = "SEL Facturation V2"
CONFIG_SCHEMA_VERSION = 1
CLIENT_ASSETS = frozenset({"index.html", "app.css", "app.js"})


def application_root() -> Path:
    return Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent))


def client_asset(name: str) -> Path:
    if name not in CLIENT_ASSETS:
        raise ValueError("Ressource client non autorisée.")
    return application_root() / "v2" / "client" / name


def data_root() -> Path:
    configured = os.environ.get("SEL_V2_DATA_DIR", "").strip()
    if configured:
        return Path(configured).expanduser()
    local_app_data = os.environ.get("LOCALAPPDATA", "").strip()
    if local_app_data:
        return Path(local_app_data) / "SEL Facturation V2"
    return Path.home() / "AppData" / "Local" / "SEL Facturation V2"


def _atomic_json_write(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{secrets.token_hex(8)}.tmp")
    try:
        temporary.write_text(
            json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def _validate_config(payload: Any, config_path: Path) -> dict[str, Any]:
    if not isinstance(payload, dict) or payload.get("schema_version") != CONFIG_SCHEMA_VERSION:
        raise ValueError("La configuration locale SEL V2 est invalide ou obsolète.")
    required = ("organization_id", "device_id", "auth_token", "database_path", "port")
    if any(not isinstance(payload.get(key), str if key != "port" else int) for key in required):
        raise ValueError("La configuration locale SEL V2 est incomplète.")
    if len(payload["auth_token"].strip()) < 16:
        raise ValueError("Le jeton local SEL V2 est trop court.")
    if not (1024 <= payload["port"] <= 65535):
        raise ValueError("Le port local SEL V2 est invalide.")
    if Path(payload["database_path"]).parent != config_path.parent:
        raise ValueError("La base locale SEL V2 doit rester dans le dossier de données.")
    return payload


def load_or_create_config(config_path: Path | None = None) -> dict[str, Any]:
    config_path = Path(config_path) if config_path else data_root() / "config.json"
    if config_path.is_file():
        return _validate_config(json.loads(config_path.read_text(encoding="utf-8")), config_path)
    config_path.parent.mkdir(parents=True, exist_ok=True)
    payload: dict[str, Any] = {
        "schema_version": CONFIG_SCHEMA_VERSION,
        "organization_id": str(uuid4()),
        "device_id": str(uuid4()),
        "auth_token": secrets.token_urlsafe(32),
        "database_path": str(config_path.parent / "sel-local-v2.sqlite"),
        "port": 8090,
    }
    _atomic_json_write(config_path, payload)
    return payload


def _sqlite_url(database_path: str) -> str:
    return "sqlite:///" + Path(database_path).absolute().as_posix().lstrip("/")


def native_binary() -> Path:
    configured = os.environ.get("SEL_LOCAL_BINARY", "").strip()
    candidates = []
    if configured:
        candidates.append(Path(configured))
    root = application_root()
    candidates.extend(
        [
            root / "sel-local.exe",
            root / "v2" / "target" / "release" / "sel-local.exe",
            root / "v2" / "bin" / "sel-local.exe",
            Path(__file__).resolve().parent / "v2" / "target" / "release" / "sel-local.exe",
        ]
    )
    for candidate in candidates:
        candidate = candidate.expanduser().resolve()
        if candidate.is_file():
            return candidate
    raise FileNotFoundError(
        "sel-local.exe est absent. Réinstallez le candidat SEL V2 ou indiquez SEL_LOCAL_BINARY."
    )


class LocalApiBridge:
    def __init__(self, base_url: str, auth_token: str):
        self.base_url = base_url.rstrip("/")
        self._auth_token = auth_token
        self._window = None

    def attach_window(self, window: Any) -> None:
        self._window = window

    def request(self, path: str, method: str = "GET", body: Any = None) -> dict[str, Any] | list[Any]:
        if not isinstance(path, str) or not path.startswith("/local/v1/") or "://" in path:
            raise ValueError("Seules les routes locales SEL V2 sont autorisées.")
        encoded = None
        headers = {"Authorization": f"Bearer {self._auth_token}"}
        if body is not None:
            encoded = json.dumps(body, ensure_ascii=False).encode("utf-8")
            headers["Content-Type"] = "application/json"
        request = Request(
            f"{self.base_url}{path}", data=encoded, headers=headers, method=method.upper()
        )
        try:
            with urlopen(request, timeout=15) as response:
                return json.loads(response.read().decode("utf-8"))
        except HTTPError as error:
            detail = error.read().decode("utf-8", errors="replace")[:500]
            raise RuntimeError(f"API locale HTTP {error.code}: {detail}") from error
        except (URLError, TimeoutError) as error:
            raise RuntimeError("Le service local SEL V2 est indisponible.") from error

    def close(self) -> dict[str, bool]:
        if self._window is not None:
            self._window.destroy()
        return {"ok": True}


class LocalRuntime:
    def __init__(self, config: dict[str, Any]):
        self.config = config
        self.process: subprocess.Popen[str] | None = None
        self.base_url = f"http://127.0.0.1:{config['port']}"

    def _environment(self) -> dict[str, str]:
        environment = os.environ.copy()
        environment.update(
            {
                "SEL_LOCAL_PORT": str(self.config["port"]),
                "SEL_LOCAL_DATABASE_URL": _sqlite_url(self.config["database_path"]),
                "SEL_ORGANIZATION_ID": self.config["organization_id"],
                "SEL_DEVICE_ID": self.config["device_id"],
                "SEL_LOCAL_AUTH_TOKEN": self.config["auth_token"],
            }
        )
        return environment

    def start(self) -> None:
        binary = native_binary()
        creationflags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
        self.process = subprocess.Popen(
            [str(binary)],
            cwd=str(binary.parent),
            env=self._environment(),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=creationflags,
            text=True,
        )
        bridge = LocalApiBridge(self.base_url, self.config["auth_token"])
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError("Le service local Rust SEL V2 s'est arrêté pendant le démarrage.")
            try:
                bridge.request("/local/v1/sync/status")
                return
            except (RuntimeError, URLError, OSError):
                time.sleep(0.25)
        self.stop()
        raise TimeoutError("Le service local Rust SEL V2 n'a pas répondu à temps.")

    def stop(self) -> None:
        if self.process is None:
            return
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        self.process = None


def packaging_smoke() -> dict[str, Any]:
    missing = [name for name in CLIENT_ASSETS if not client_asset(name).is_file()]
    if missing:
        raise FileNotFoundError(f"Ressources V2 absentes: {', '.join(sorted(missing))}")
    return {"ok": True, "client_assets": len(CLIENT_ASSETS), "native_binary": "deferred-to-build"}


def _startup_message(error: Exception) -> str:
    return f"SEL Facturation V2 n'a pas pu démarrer : {str(error).strip() or type(error).__name__}"


def main(argv: list[str] | None = None) -> int:
    arguments = list(sys.argv[1:] if argv is None else argv)
    if arguments == ["--sel-v2-packaging-smoke"]:
        try:
            packaging_smoke()
            return 0
        except Exception:
            logging.exception("Échec du smoke test SEL V2")
            return 1

    runtime: LocalRuntime | None = None
    try:
        config = load_or_create_config()
        runtime = LocalRuntime(config)
        runtime.start()
        bridge = LocalApiBridge(runtime.base_url, config["auth_token"])
        import webview

        window = webview.create_window(
            APP_TITLE,
            client_asset("index.html").as_uri(),
            js_api=bridge,
            width=1360,
            height=860,
            min_size=(1100, 700),
            text_select=True,
        )
        bridge.attach_window(window)
        webview.start(debug=False)
        return 0
    except Exception as error:
        logging.exception("Démarrage SEL V2 impossible")
        if os.name == "nt":
            try:
                import ctypes

                ctypes.windll.user32.MessageBoxW(None, _startup_message(error), APP_TITLE, 0x10)
            except Exception:
                pass
        return 1
    finally:
        if runtime is not None:
            runtime.stop()


if __name__ == "__main__":
    raise SystemExit(main())
