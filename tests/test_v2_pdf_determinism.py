from __future__ import annotations

from pathlib import Path

from pypdf import PdfReader, PdfWriter

from ttyinv.pdf_hardening import normalize_pdf


def source_pdf(path: Path) -> None:
    writer = PdfWriter()
    writer.add_blank_page(width=595, height=842)
    writer.add_metadata({"/CreationDate": "D:20991231235959Z", "/Producer": "volatile"})
    with path.open("wb") as handle:
        writer.write(handle)


def test_pdf_normalization_is_deterministic(tmp_path: Path) -> None:
    source = tmp_path / "source.pdf"
    first = tmp_path / "first.pdf"
    second = tmp_path / "second.pdf"
    source_pdf(source)
    seed = b"0123456789abcdef" * 2
    normalize_pdf(source, first, seed=seed)
    normalize_pdf(source, second, seed=seed)
    assert first.read_bytes() == second.read_bytes()
    metadata = PdfReader(str(first)).metadata
    assert metadata.creation_date.year == 2000
    assert metadata.producer == "ttyinv"
