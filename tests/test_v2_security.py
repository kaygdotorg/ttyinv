from __future__ import annotations

from pathlib import Path

from ttyinv.security import resolve_local_reference, validate_local_references


def test_relative_asset_inside_invoice_root_is_allowed(tmp_path: Path) -> None:
    asset = tmp_path / "assets" / "mark.svg"
    asset.parent.mkdir()
    asset.write_text("<svg/>", encoding="utf-8")
    resolved, error = resolve_local_reference("./assets/mark.svg", root=tmp_path, allow_outside_root=False)
    assert error is None
    assert resolved == asset.resolve()


def test_parent_traversal_is_rejected(tmp_path: Path) -> None:
    root = tmp_path / "invoice"
    root.mkdir()
    resolved, error = resolve_local_reference("../secret.txt", root=root, allow_outside_root=False)
    assert resolved == (tmp_path / "secret.txt").resolve()
    assert error == "path escapes the invoice directory"


def test_traversal_can_be_explicitly_allowed(tmp_path: Path) -> None:
    root = tmp_path / "invoice"
    root.mkdir()
    _, error = resolve_local_reference("../shared/terms.pdf", root=root, allow_outside_root=True)
    assert error is None


def test_missing_markdown_image_is_an_error(tmp_path: Path) -> None:
    source_path = tmp_path / "invoice.md"
    source = "![Company mark](./missing.svg)\n"
    diagnostics = validate_local_references(source_path, source)
    assert any(item.code == "PATH002" and item.severity == "error" for item in diagnostics)


def test_missing_local_document_link_is_optional_warning(tmp_path: Path) -> None:
    source_path = tmp_path / "invoice.md"
    source = "[Previous invoice](./previous.pdf)\n"
    assert not validate_local_references(source_path, source)
    diagnostics = validate_local_references(source_path, source, require_link_targets=True)
    assert any(item.code == "PATH002" and item.severity == "warning" for item in diagnostics)
