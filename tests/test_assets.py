from __future__ import annotations

from pathlib import Path

import pytest

from ttyinv.assets import embed_asset_path, embed_local_asset
from ttyinv.errors import TtyinvError


def test_embeds_supported_asset_below_invoice_directory(tmp_path: Path) -> None:
    asset = tmp_path / "assets" / "mark.svg"
    asset.parent.mkdir()
    asset.write_text("<svg/>", encoding="utf-8")

    assert embed_local_asset("assets/mark.svg", tmp_path).startswith("data:image/svg+xml;base64,")


def test_rejects_parent_traversal(tmp_path: Path) -> None:
    invoice_directory = tmp_path / "invoice"
    invoice_directory.mkdir()
    (tmp_path / "secret.png").write_bytes(b"not really an image")

    with pytest.raises(TtyinvError, match="escapes the invoice directory"):
        embed_local_asset("../secret.png", invoice_directory)


def test_rejects_symlink_that_escapes_invoice_directory(tmp_path: Path) -> None:
    invoice_directory = tmp_path / "invoice"
    invoice_directory.mkdir()
    outside = tmp_path / "outside.svg"
    outside.write_text("<svg/>", encoding="utf-8")
    (invoice_directory / "mark.svg").symlink_to(outside)

    with pytest.raises(TtyinvError, match="escapes the invoice directory"):
        embed_local_asset("mark.svg", invoice_directory)


def test_rejects_guessed_non_asset_mime_type(tmp_path: Path) -> None:
    document = tmp_path / "private.txt"
    document.write_text("private", encoding="utf-8")

    with pytest.raises(TtyinvError, match="unsupported file type"):
        embed_asset_path(document)


def test_rejects_configured_asset_path_outside_invoice_directory(tmp_path: Path) -> None:
    invoice_directory = tmp_path / "invoice"
    invoice_directory.mkdir()
    outside = tmp_path / "private-font.woff2"
    outside.write_bytes(b"not really a font")

    with pytest.raises(TtyinvError, match="escapes the invoice directory"):
        embed_local_asset(str(outside), invoice_directory)
