"""PDF generation and deterministic metadata normalization."""

from __future__ import annotations

import hashlib
import tempfile
from pathlib import Path

from pypdf import PdfReader, PdfWriter
from pypdf.generic import ArrayObject, ByteStringObject


def render_pdf(html: str, output_path: Path, *, deterministic: bool = False, browser_executable: str | None = None) -> None:
    try:
        from playwright.sync_api import sync_playwright
    except ImportError as exc:  # pragma: no cover
        raise RuntimeError("PDF output requires Playwright") from exc
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="ttyinv-pdf-") as directory:
        html_path = Path(directory) / "invoice.html"
        raw_pdf = Path(directory) / "invoice.raw.pdf"
        html_path.write_text(html, encoding="utf-8")
        with sync_playwright() as playwright:
            options: dict[str, object] = {"headless": True}
            if browser_executable:
                options["executable_path"] = browser_executable
            browser = playwright.chromium.launch(**options)
            try:
                page = browser.new_page(locale="en-GB")
                page.goto(html_path.as_uri(), wait_until="networkidle")
                page.evaluate("document.fonts.ready")
                page.emulate_media(media="print")
                page.pdf(
                    path=str(raw_pdf), format="A4", print_background=True,
                    prefer_css_page_size=True,
                    margin={"top": "0", "right": "0", "bottom": "0", "left": "0"},
                )
            finally:
                browser.close()
        if deterministic:
            normalize_pdf(raw_pdf, output_path, seed=hashlib.sha256(html.encode()).digest())
        else:
            output_path.write_bytes(raw_pdf.read_bytes())


def normalize_pdf(source: Path, destination: Path, *, seed: bytes) -> None:
    reader = PdfReader(str(source))
    writer = PdfWriter()
    for page in reader.pages:
        writer.add_page(page)
    writer.add_metadata({
        "/Title": "Invoice", "/Author": "", "/Subject": "", "/Keywords": "",
        "/Creator": "ttyinv", "/Producer": "ttyinv",
        "/CreationDate": "D:20000101000000Z", "/ModDate": "D:20000101000000Z",
    })
    stable_id = ByteStringObject(seed[:16])
    writer._ID = ArrayObject([stable_id, stable_id])
    with destination.open("wb") as handle:
        writer.write(handle)
