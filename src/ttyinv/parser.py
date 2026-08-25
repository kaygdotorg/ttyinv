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
from .yaml_support import StringDateSafeLoader

_FRONTMATTER_RE = re.compile(r"\A\ufeff?---\s*\r?\n(?P<yaml>[\s\S]*?)\r?\n---\s*(?:\r?\n|\Z)")
_BR_RE = re.compile(r"&lt;br\s*/?&gt;", re.IGNORECASE)
_PAGE_BREAK_LINE_RE = re.compile(r"^[ \t]*<!-- ttyinv:page-break-before -->[ \t]*$")
_SUMMARY_ONLY_LINE_RE = re.compile(r"^[ \t]*<!-- ttyinv:summary-only -->[ \t]*$")
_H2_LINE_RE = re.compile(r"^[ \t]{0,3}##[ \t]+")
_FENCE_START_RE = re.compile(r"^[ \t]{0,3}(`{3,}|~{3,})")


def _safe_validation_message(exc: ValidationError) -> str:
    for error in exc.errors(include_url=False):
        if error.get("msg") == "Value error, due date must be on or after issue date":
            return "due date must be on or after issue date"
    return "Invalid invoice frontmatter."

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


def _page_break_indexes(body: str) -> set[int]:
    """Return section ordinals whose H2 is immediately preceded by the marker.

    The source is scanned once, in heading order.  In particular, this does not
    search all headings for every marker: repeated section titles therefore
    remain independent and the mapping stays linear for large invoices.
    """

    return _directive_indexes(body)[0]


def _summary_only_indexes(body: str) -> set[int]:
    """Return section ordinals whose H2 is immediately preceded by the marker."""

    return _directive_indexes(body)[1]


def _directive_indexes(body: str) -> tuple[set[int], set[int]]:
    """Bind adjacent directive lines to the next H2 in one linear scan."""

    page_indexes: set[int] = set()
    summary_indexes: set[int] = set()
    heading_position = -1
    lines = body.splitlines()
    fence: tuple[str, int] | None = None
    pending_page = False
    pending_summary = False
    for line in lines:
        if fence is not None:
            fence_character, fence_length = fence
            if re.fullmatch(rf"[ \t]{{0,3}}{re.escape(fence_character)}{{{fence_length},}}[ \t]*", line):
                fence = None
            continue
        if fence_match := _FENCE_START_RE.match(line):
            fence = (fence_match.group(1)[0], len(fence_match.group(1)))
            continue
        if _PAGE_BREAK_LINE_RE.fullmatch(line):
            pending_page = True
            continue
        if _SUMMARY_ONLY_LINE_RE.fullmatch(line):
            pending_summary = True
            continue
        if _H2_LINE_RE.match(line):
            heading_position += 1
            if pending_page:
                page_indexes.add(heading_position)
            if pending_summary:
                summary_indexes.add(heading_position)
        pending_page = False
        pending_summary = False
    return page_indexes, summary_indexes


def _strip_directive_markers(body: str) -> str:
    """Remove recognized directive lines outside fenced Markdown code."""

    kept: list[str] = []
    fence: tuple[str, int] | None = None
    for raw_line in body.splitlines(keepends=True):
        line = raw_line.rstrip("\r\n")
        if fence is not None:
            fence_character, fence_length = fence
            if re.fullmatch(rf"[ \t]{{0,3}}{re.escape(fence_character)}{{{fence_length},}}[ \t]*", line):
                fence = None
            kept.append(raw_line)
            continue
        if fence_match := _FENCE_START_RE.match(line):
            fence = (fence_match.group(1)[0], len(fence_match.group(1)))
            kept.append(raw_line)
            continue
        if _PAGE_BREAK_LINE_RE.fullmatch(line) or _SUMMARY_ONLY_LINE_RE.fullmatch(line):
            continue
        kept.append(raw_line)
    return "".join(kept)


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


def _split_sections(
    md: MarkdownIt,
    tokens: list[Token],
    page_break_indexes: set[int],
    summary_only_indexes: set[int],
) -> tuple[str, list[FinancialSection | ProseSection]]:
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
            trailing = [token for token in content[close_index + 1 :] if token.type != "fence"]
            if trailing:
                raise TtyinvError(
                    f"Financial section {title!r} must contain only one table. Put notes in a separate section."
                )
            sections.append(
                FinancialSection(
                    title=title,
                    table=_parse_table(md, content, title),
                    page_break_before=heading_position in page_break_indexes,
                    summary_only=heading_position in summary_only_indexes,
                )
            )
        else:
            sections.append(
                ProseSection(
                    title=title,
                    html=_render_tokens(md, content),
                    page_break_before=heading_position in page_break_indexes,
                    summary_only=heading_position in summary_only_indexes,
                )
            )

    if not any(section.kind == "financial" for section in sections):
        raise TtyinvError("Invoice must contain at least one level-two heading followed by a table.")
    return preamble_html, sections


def parse_invoice_file(source_path: str | Path) -> ParsedInvoice:
    absolute_path = Path(source_path).expanduser().resolve()
    try:
        source = absolute_path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise TtyinvError("Cannot read invoice.") from exc

    match = _FRONTMATTER_RE.match(source)
    if not match:
        raise TtyinvError("Invoice must begin with YAML frontmatter delimited by --- lines.")

    try:
        raw_frontmatter: Any = yaml.load(match.group("yaml"), Loader=StringDateSafeLoader)
    except (yaml.YAMLError, RecursionError) as exc:
        raise TtyinvError("Invalid YAML frontmatter.") from exc

    try:
        frontmatter = InvoiceFrontmatter.model_validate(raw_frontmatter)
    except (ValidationError, RecursionError) as exc:
        message = _safe_validation_message(exc) if isinstance(exc, ValidationError) else "Invalid invoice frontmatter."
        raise TtyinvError(message) from exc

    body = source[match.end() :]
    page_break_indexes, summary_only_indexes = _directive_indexes(body)
    body = _strip_directive_markers(body)
    md = _markdown()
    tokens = md.parse(body)
    preamble_html, sections = _split_sections(md, tokens, page_break_indexes, summary_only_indexes)

    return ParsedInvoice(
        source_path=absolute_path,
        source_directory=absolute_path.parent,
        frontmatter=frontmatter,
        preamble_html=preamble_html,
        sections=sections,
    )
