"""Build the Windows application icon from the official SEL SVG asset."""

from __future__ import annotations

import base64
import io
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "packaging" / "sel-icon-source.svg"
TARGET = ROOT / "packaging" / "sel-facturation.ico"
STATIC_TARGET = ROOT / "static" / "sel-icon.png"
SIZES = (16, 24, 32, 48, 64, 128, 256)


def read_embedded_png_bytes(source: Path) -> bytes:
    content = source.read_text(encoding="utf-8")
    marker = "data:image/png;base64,"
    if marker not in content:
        raise ValueError(f"{source} ne contient pas de PNG embarqué.")
    payload = content.split(marker, 1)[1].split('"', 1)[0]
    return base64.b64decode(payload)


def prepare_icon(image: Image.Image) -> Image.Image:
    alpha = image.getchannel("A")
    # Remove only fully transparent margins while retaining anti-aliased edges.
    bbox = alpha.point(lambda value: 255 if value > 8 else 0).getbbox()
    if bbox:
        image = image.crop(bbox)
    side = max(image.size)
    canvas = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    image.thumbnail((side, side), Image.Resampling.LANCZOS)
    canvas.alpha_composite(image, ((side - image.width) // 2, (side - image.height) // 2))
    return canvas


def main() -> None:
    source_bytes = read_embedded_png_bytes(SOURCE)
    source_image = Image.open(io.BytesIO(source_bytes)).convert("RGBA")
    STATIC_TARGET.parent.mkdir(parents=True, exist_ok=True)
    TARGET.parent.mkdir(parents=True, exist_ok=True)
    STATIC_TARGET.write_bytes(source_bytes)
    icon = prepare_icon(source_image)
    icon.save(TARGET, format="ICO", sizes=[(size, size) for size in SIZES])
    print(f"Icône SEL synchronisée : {STATIC_TARGET}")
    print(f"Icône Windows SEL générée : {TARGET}")


if __name__ == "__main__":
    main()
