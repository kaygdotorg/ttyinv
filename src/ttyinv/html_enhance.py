"""Post-render hardening for self-contained ttyinv HTML."""

from __future__ import annotations

import base64
import hashlib
import os
import re
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlparse

from bs4 import BeautifulSoup, Tag

from .appearance import Palette
from .security import resolve_local_reference


@dataclass(frozen=True, slots=True)
class EnhanceOptions:
    source_path: Path
    output_path: Path
    palette: Palette
    density: str = "comfortable"
    allow_outside_root: bool = False
    deterministic: bool = False


def _classes(tag: Tag) -> str:
    value = tag.get("class", [])
    return value if isinstance(value, str) else " ".join(str(item) for item in value)


def _ensure_frame(soup: BeautifulSoup) -> None:
    """Keep one frame model: a dashed rectangle plus four literal `+` glyphs.

    The visual renderer normally emits this structure already.  The fallback is
    only for older/self-authored HTML; hardening must never introduce a second,
    vector-junction frame that can drift from the calibrated renderer.
    """

    if isinstance(soup.select_one(".page-frame"), Tag):
        return
    if not isinstance(soup.body, Tag):
        return

    frame = soup.new_tag("div")
    frame["class"] = ["page-frame"]
    frame["aria-hidden"] = "true"
    soup.body.insert(0, frame)

    for name in ("tl", "tr", "bl", "br"):
        corner = soup.new_tag("span")
        corner["class"] = ["frame-corner", name]
        corner["aria-hidden"] = "true"
        corner.string = "+"
        frame.insert_after(corner)


def _heading(table: Tag) -> str:
    previous = table.find_previous(["h1", "h2", "h3", "caption"])
    if isinstance(previous, Tag):
        text = previous.get_text(" ", strip=True).strip("[] ")
        if text:
            return text
    return "Invoice items"


def _enhance_semantics(soup: BeautifulSoup) -> None:
    for heading in soup.find_all(["h1", "h2", "h3"]):
        if isinstance(heading, Tag):
            text, classes = heading.get_text(" ", strip=True), _classes(heading).casefold()
            if "section" in classes or (text.startswith("[") and text.endswith("]")):
                heading["data-ttyinv-section-label"] = ""
    for index, table in enumerate(soup.find_all("table"), start=1):
        if not isinstance(table, Tag):
            continue
        title = _heading(table)
        table["data-ttyinv-table"] = str(index)
        table["aria-label"] = title
        if not isinstance(table.find("caption", recursive=False), Tag):
            caption = soup.new_tag("caption")
            caption["class"] = ["ttyinv-sr-only"]
            caption.string = title
            table.insert(0, caption)
        for header in table.find_all("th"):
            if isinstance(header, Tag) and not header.get("scope"):
                header["scope"] = "col"
        for row in table.find_all("tr"):
            if isinstance(row, Tag):
                row["data-ttyinv-row"] = ""
        for footer in table.find_all("tfoot"):
            if isinstance(footer, Tag):
                footer["data-ttyinv-total"] = ""
    for tag in soup.find_all(True):
        if isinstance(tag, Tag) and any(token in _classes(tag).casefold() for token in ("grand-total", "total-due", "totals")):
            tag["data-ttyinv-total"] = ""
    for image in soup.find_all("img"):
        if not isinstance(image, Tag) or image.has_attr("alt"):
            continue
        classes = _classes(image).casefold()
        image["alt"] = "Signature" if "signature" in classes else "Company logo" if "logo" in classes or "brand" in classes else ""


def _rewrite_links(soup: BeautifulSoup, options: EnhanceOptions) -> None:
    source_root, output_root = options.source_path.parent.resolve(), options.output_path.parent.resolve()
    for link in soup.find_all("a"):
        if not isinstance(link, Tag):
            continue
        href = str(link.get("href", "")).strip()
        if not href:
            continue
        parsed = urlparse(href)
        if parsed.scheme in {"http", "https"}:
            link["rel"] = "noopener noreferrer"
            continue
        if parsed.scheme in {"mailto", "tel"} or href.startswith("#"):
            continue
        if parsed.scheme:
            del link["href"]
            link["data-ttyinv-blocked-link"] = parsed.scheme.casefold()
            continue
        resolved, error = resolve_local_reference(href, root=source_root, allow_outside_root=options.allow_outside_root)
        if error or resolved is None:
            if error:
                del link["href"]
                link["data-ttyinv-blocked-link"] = "local-path"
            continue
        fragment = f"#{parsed.fragment}" if parsed.fragment else ""
        try:
            rewritten = Path(os.path.relpath(resolved, output_root)).as_posix()
        except ValueError:
            rewritten = resolved.as_uri()
        link["href"] = rewritten + fragment
        link["data-ttyinv-local-link"] = "best-effort-pdf"


def _font_digest(html: str) -> str | None:
    digests = []
    for encoded in re.findall(r"data:[^;,]+(?:;charset=[^;,]+)?;base64,([A-Za-z0-9+/=]+)", html):
        try:
            payload = base64.b64decode(encoded, validate=True)
        except Exception:
            continue
        if payload[:4] in {b"wOFF", b"wOF2", b"OTTO", b"\x00\x01\x00\x00"}:
            digests.append(hashlib.sha256(payload).hexdigest())
    return hashlib.sha256("\n".join(sorted(digests)).encode()).hexdigest() if digests else None


def _css(options: EnhanceOptions) -> str:
    # Comfortable is the calibrated renderer and therefore gets *no* typography
    # override here. Compact is opt-in and may reduce density explicitly.
    compact_css = ""
    if options.density == "compact":
        compact_css = "body{font-size:7.65pt;line-height:1.37;}"
    return f"""
:root {{ --paper:{options.palette.paper}!important; --ink:{options.palette.ink}!important; --muted:{options.palette.muted}!important; --accent:{options.palette.accent}!important; }}
{compact_css}
.ttyinv-sr-only {{ position:absolute!important;width:1px!important;height:1px!important;padding:0!important;margin:-1px!important;overflow:hidden!important;clip:rect(0,0,0,0)!important;white-space:nowrap!important;border:0!important; }}
table {{ border-collapse:collapse;table-layout:fixed; }}
table,thead,tbody,tfoot,tr,th,td {{ border-left:0!important;border-right:0!important; }}
thead {{ display:table-header-group; }}
tfoot {{ display:table-footer-group;break-inside:avoid-page;page-break-inside:avoid; }}
tr,[data-ttyinv-row],.invoice-row {{ break-inside:avoid-page;page-break-inside:avoid; }}
section>h2,.section-label,.section-heading,.invoice-section-title {{ break-after:avoid-page;page-break-after:avoid; }}
.invoice-section table,.financial-section table {{ break-before:avoid-page;page-break-before:avoid; }}
.totals,.grand-total,.total-due,.footer-stack,.payment-section,.signature,.invoice-closing {{ break-inside:avoid-page;page-break-inside:avoid; }}
a {{ color:var(--accent);text-decoration-thickness:.08em;text-underline-offset:.18em; }}
@media print {{ html,body{{background:var(--paper)!important;print-color-adjust:exact;-webkit-print-color-adjust:exact}} thead{{display:table-header-group!important}}tfoot{{display:table-footer-group!important}} }}
""".strip()


def enhance_html(html: str, options: EnhanceOptions) -> str:
    soup = BeautifulSoup(html, "html.parser")
    if soup.html:
        soup.html["lang"] = soup.html.get("lang") or "en"
        soup.html["data-ttyinv-density"] = options.density
    if not isinstance(soup.find("main"), Tag) and isinstance(soup.body, Tag):
        main = soup.new_tag("main")
        main["role"] = "document"
        for child in list(soup.body.contents):
            main.append(child.extract())
        soup.body.append(main)
    elif isinstance(soup.find("main"), Tag):
        soup.find("main")["role"] = soup.find("main").get("role") or "document"
    _ensure_frame(soup)
    _enhance_semantics(soup)
    _rewrite_links(soup, options)
    head = soup.head
    if not isinstance(head, Tag):
        head = soup.new_tag("head")
        soup.html.insert(0, head)
    style = soup.new_tag("style", id="ttyinv-v2-hardening")
    style.string = _css(options)
    head.append(style)
    generator = soup.new_tag("meta")
    generator["name"], generator["content"] = "generator", "ttyinv 0.2"
    head.append(generator)
    rendered = str(soup)
    if digest := _font_digest(rendered):
        meta = soup.new_tag("meta")
        meta["name"], meta["content"] = "ttyinv-font-sha256", digest
        head.append(meta)
        rendered = str(soup)
    if options.deterministic:
        soup = BeautifulSoup(rendered, "html.parser")
        for meta in soup.find_all("meta"):
            if isinstance(meta, Tag) and str(meta.get("name", "")).casefold() in {"date", "created", "modified", "creation-date"}:
                meta.decompose()
        marker = soup.new_tag("meta")
        marker["name"], marker["content"] = "ttyinv-deterministic", "true"
        soup.head.append(marker)
        rendered = str(soup)
    return rendered
