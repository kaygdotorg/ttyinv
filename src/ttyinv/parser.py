from __future__ import annotations

import html
import re
from pathlib import Path
from typing import Any

import yaml
from markdown_it import MarkdownIt
from markdown_it.token import Token
from pydantic import ValidationError

from .errors import TtyinvError
from .models import (
    FinancialSection,
    InvoiceFrontmatter,
    ParsedInvoice,
    ParsedTable,
    ProseSection,
    TableCell,
)

_FRONTMATTER_RE = re.compile(r"\A\ufeff?---\s*\r?\n(?P<yaml>[\s\S]*?)\r?\n---\s*(?:\r?\n|\Z)")
_BR_RE = re.compile(r"&lt;br\s*/?&gt;", re.IGNORECASE)


def _markdown() -> MarkdownIt:
    return MarkdownIt(
        "commonmark",
        {
            "html": False,
            "linkify": False,
            "typographer": False,
        },
    ).enable("table")


def _render_inline(md: MarkdownIt, token: Token) -> str:
    rendered = md.renderer.renderInline(token.children or [], md.options, {})
    return _BR_RE.sub("<br>", rendered)


def _render_tokens(md: MarkdownIt, tokens: list[Token]) -> str:
    rendered = md.renderer.render(tokens, md.options, {})
    return _BR_RE.sub("<br>", rendered)


def _alignment(token: Token) -> str | None:
    style = token.attrGet("style") or ""
    if "text-align:right" in style:
        return "right"
    if "text-align:center" in style:
        return "center"
    if "text-align:left" in style:
        return "left"
    return None


def _parse_table(md: MarkdownIt, tokens: list[Token], section_title: str) -> ParsedTable:
    headers: list[TableCell] = []
    rows: list[list[TableCell]] = []
    align: list[str | None] = []
    current_row: list[TableCell] | None = None
    in_header = False
    cell_open: Token | None = None

    for index, token in enumerate(tokens):
        if token.type == "thead_open":
            in_header = True
        elif token.type == "thead_close":
            in_header = False
        elif token.type == "tr_open":
            current_row = []
        elif token.type == "tr_close":
            if current_row is None:
                continue
            if in_header:
                headers = current_row
            else:
                rows.append(current_row)
            current_row = None
        elif token.type in {"th_open", "td_open"}:
            cell_open = token
        elif token.type == "inline" and cell_open is not None and current_row is not None:
            cell = TableCell(source=token.content.strip(), html=_render_inline(md, token))
            current_row.append(cell)
            if in_header:
                align.append(_alignment(cell_open))
            cell_open = None

    if len(headers) < 2:
        raise TtyinvError(f"Section {section_title!r} must have a table heading row with at least two columns.")
    if not rows:
        raise TtyinvError(f"Section {section_title!r} must contain at least one table row.")
    for row_number, row in enumerate(rows, start=1):
        if len(row) != len(headers):
            raise TtyinvError(
                f"Section {section_title!r}, row {row_number}: found {len(row)} cells; expected {len(headers)}."
            )
    if len(align) < len(headers):
        align.extend([None] * (len(headers) - len(align)))
    return ParsedTable(headers=headers, align=align[: len(headers)], rows=rows)


def _split_sections(md: MarkdownIt, tokens: list[Token]) -> tuple[str, list[FinancialSection | ProseSection]]:
    heading_indexes = [
        index
        for index, token in enumerate(tokens)
        if token.type == "heading_open" and token.tag == "h2"
    ]
    if not heading_indexes:
        raise TtyinvError("Invoice must contain at least one level-two heading followed by a table.")

    preamble_html = _render_tokens(md, tokens[: heading_indexes[0]])
    sections: list[FinancialSection | ProseSection] = []

    for heading_position, heading_index in enumerate(heading_indexes):
        title_token = tokens[heading_index + 1] if heading_index + 1 < len(tokens) else None
        if title_token is None or title_token.type != "inline" or not title_token.content.strip():
            raise TtyinvError("Level-two section headings cannot be empty.")
        title = title_token.content.strip()
        content_start = heading_index + 3
        content_end = (
            heading_indexes[heading_position + 1]
            if heading_position + 1 < len(heading_indexes)
            else len(tokens)
        )
        content = tokens[content_start:content_end]

        if content and content[0].type == "table_open":
            table_close_indexes = [index for index, token in enumerate(content) if token.type == "table_close"]
            if len(table_close_indexes) != 1:
                raise TtyinvError(
                    f"Financial section {title!r} must contain exactly one table. Use one level-two heading per table."
                )
            close_index = table_close_indexes[0]
            trailing = content[close_index + 1 :]
            if trailing:
                raise TtyinvError(
                    f"Financial section {title!r} must contain only one table. Put notes in a separate section."
                )
            sections.append(FinancialSection(title=title, table=_parse_table(md, content, title)))
        else:
            sections.append(ProseSection(title=title, html=_render_tokens(md, content)))

    if not any(section.kind == "financial" for section in sections):
        raise TtyinvError("Invoice must contain at least one level-two heading followed by a table.")
    return preamble_html, sections


def parse_invoice_file(source_path: str | Path) -> ParsedInvoice:
    absolute_path = Path(source_path).expanduser().resolve()
    try:
        source = absolute_path.read_text(encoding="utf-8")
    except OSError as exc:
        raise TtyinvError(f"Cannot read invoice {absolute_path}: {exc}") from exc

    match = _FRONTMATTER_RE.match(source)
    if not match:
        raise TtyinvError("Invoice must begin with YAML frontmatter delimited by --- lines.")

    try:
        raw_frontmatter: Any = yaml.safe_load(match.group("yaml"))
    except yaml.YAMLError as exc:
        raise TtyinvError(f"Invalid YAML frontmatter: {exc}") from exc

    try:
        frontmatter = InvoiceFrontmatter.model_validate(raw_frontmatter)
    except ValidationError as exc:
        raise TtyinvError(f"Invalid invoice frontmatter:\n{exc}") from exc

    body = source[match.end() :]
    md = _markdown()
    tokens = md.parse(body)
    preamble_html, sections = _split_sections(md, tokens)

    return ParsedInvoice(
        source_path=absolute_path,
        source_directory=absolute_path.parent,
        frontmatter=frontmatter,
        preamble_html=preamble_html,
        sections=sections,
    )
