from __future__ import annotations

from pathlib import Path

from bs4 import BeautifulSoup

from ttyinv.appearance import resolve_palette
from ttyinv.html_enhance import EnhanceOptions, enhance_html


HTML = """<!doctype html><html><head></head><body>
<h2 class="section-heading">[ Contract fees ]</h2>
<table><thead><tr><th>Description</th><th>Amount</th></tr></thead>
<tbody><tr><td>Consulting</td><td>200.00</td></tr></tbody></table>
<a href="./previous.pdf">Previous invoice</a>
</body></html>"""


def test_enhancer_adds_accessible_semantics_and_frame(tmp_path: Path) -> None:
    source = tmp_path / "invoice.md"
    output = tmp_path / "build" / "invoice.html"
    rendered = enhance_html(HTML, EnhanceOptions(source, output, resolve_palette("light")))
    soup = BeautifulSoup(rendered, "html.parser")
    assert soup.html["lang"] == "en"
    assert soup.find("main")["role"] == "document"
    assert soup.find("table")["aria-label"] == "Contract fees"
    assert soup.find("th")["scope"] == "col"
    assert len(soup.select("[data-ttyinv-frame-line]")) == 4
    assert len(soup.select("[data-ttyinv-frame-corner]")) == 4


def test_enhancer_keeps_tables_borderless_and_adds_pagination_contract(tmp_path: Path) -> None:
    rendered = enhance_html(
        HTML,
        EnhanceOptions(tmp_path / "invoice.md", tmp_path / "invoice.html", resolve_palette("dark"), density="compact"),
    )
    assert "border-left:0!important" in rendered
    assert "thead{display:table-header-group!important}" in rendered.replace(" ", "")
    assert 'data-ttyinv-density="compact"' in rendered


def test_relative_links_are_rewritten_from_output_directory(tmp_path: Path) -> None:
    source = tmp_path / "source" / "invoice.md"
    target = source.parent / "previous.pdf"
    target.parent.mkdir()
    target.write_bytes(b"not-a-real-pdf")
    output = tmp_path / "build" / "invoice.html"
    rendered = enhance_html(HTML, EnhanceOptions(source, output, resolve_palette("light")))
    soup = BeautifulSoup(rendered, "html.parser")
    link = soup.find("a")
    assert link["href"].endswith("source/previous.pdf")
    assert link["data-ttyinv-local-link"] == "best-effort-pdf"
