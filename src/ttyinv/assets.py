from __future__ import annotations

import base64
import mimetypes
from pathlib import Path
from urllib.parse import urlparse

from .errors import TtyinvError

_SUPPORTED_MIME = {
    ".svg": "image/svg+xml",
    ".png": "image/png",
    ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg",
    ".webp": "image/webp",
    ".woff": "font/woff",
    ".woff2": "font/woff2",
    ".ttf": "font/ttf",
    ".otf": "font/otf",
}


def is_external(reference: str) -> bool:
    parsed = urlparse(reference)
    return parsed.scheme in {"http", "https", "mailto", "data"} or reference.startswith("#")


def data_url(contents: bytes, mime: str) -> str:
    encoded = base64.b64encode(contents).decode("ascii")
    return f"data:{mime};base64,{encoded}"


def embed_asset_path(asset_path: Path) -> str:
    mime = _SUPPORTED_MIME.get(asset_path.suffix.lower()) or mimetypes.guess_type(asset_path.name)[0]
    if not mime:
        raise TtyinvError(f"Cannot embed asset {asset_path}: unsupported file type.")
    try:
        contents = asset_path.read_bytes()
    except OSError as exc:
        raise TtyinvError(f"Cannot read asset {asset_path}: {exc}") from exc
    return data_url(contents, mime)


def embed_local_asset(reference: str, source_directory: Path) -> str:
    if reference.startswith("data:"):
        return reference
    if is_external(reference):
        raise TtyinvError(
            f"Remote asset {reference!r} cannot be embedded. Use a local path so the HTML remains self-contained."
        )

    asset_path = (source_directory / reference).resolve()
    try:
        return embed_asset_path(asset_path)
    except TtyinvError as exc:
        raise TtyinvError(f"Cannot embed asset {reference!r}: {exc}") from exc
