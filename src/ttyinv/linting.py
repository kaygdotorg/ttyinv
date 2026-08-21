"""Source-aware validation used by ``ttyinv lint`` and pre-render checks."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Any, Iterable

import yaml
from yaml.constructor import ConstructorError

from .appearance import contrast_diagnostics, resolve_palette
from .diagnostics import Diagnostic
from .security import validate_local_references

_FRONT = re.compile(r"\A---[ \t]*\r?\n(?P<yaml>.*?)\r?\n---[ \t]*(?:\r?\n|\Z)", re.DOTALL)
_H2 = re.compile(r"^##[ \t]+(?P<title>.+?)[ \t]*$")
_SEPARATOR = re.compile(r"^:?-{3,}:?$")
_AMOUNT = re.compile(r"^amount(?:\s*\(([A-Z]{3})\))?$", re.I)
_QTY = {"qty", "quantity", "days", "hours", "units"}
_RATE = {"rate", "unit price", "unit_price", "price"}


class UniqueKeyLoader(yaml.SafeLoader):
    pass


def _construct_mapping(loader: UniqueKeyLoader, node: yaml.MappingNode, deep: bool = False) -> dict[Any, Any]:
    mapping: dict[Any, Any] = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in mapping:
            raise ConstructorError("while constructing a mapping", node.start_mark, f"found duplicate key {key!r}", key_node.start_mark)
        mapping[key] = loader.construct_object(value_node, deep=deep)
    return mapping


UniqueKeyLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _construct_mapping)


@dataclass(frozen=True, slots=True)
class Row:
    cells: tuple[str, ...]
    line: int


@dataclass(frozen=True, slots=True)
class Table:
    title: str
    heading_line: int
    headers: tuple[str, ...]
    rows: tuple[Row, ...]


def _split(line: str) -> list[str]:
    value = line.strip()
    if value.startswith("|"):
        value = value[1:]
    if value.endswith("|") and not value.endswith(r"\|"):
        value = value[:-1]
    cells, current, escaped = [], [], False
    for char in value:
        if escaped:
            current.append(char)
            escaped = False
        elif char == "\\":
            escaped = True
            current.append(char)
        elif char == "|":
            cells.append("".join(current).strip())
            current = []
        else:
            current.append(char)
    cells.append("".join(current).strip())
    return cells


def _separator(line: str, count: int) -> bool:
    cells = _split(line)
    return len(cells) == count and all(_SEPARATOR.fullmatch(cell.replace(" ", "")) for cell in cells)


def parse_tables(body: str, first_line: int) -> tuple[list[Table], list[Diagnostic]]:
    lines = body.splitlines()
    result, diagnostics = [], []
    index = 0
    while index < len(lines):
        heading = _H2.match(lines[index])
        if not heading:
            index += 1
            continue
        end = index + 1
        while end < len(lines) and not _H2.match(lines[end]):
            end += 1
        cursor = index + 1
        while cursor + 1 < end:
            headers = _split(lines[cursor]) if "|" in lines[cursor] else []
            if headers and _separator(lines[cursor + 1], len(headers)):
                rows, row_index = [], cursor + 2
                while row_index < end and lines[row_index].strip() and "|" in lines[row_index]:
                    cells = _split(lines[row_index])
                    line = first_line + row_index
                    if len(cells) != len(headers):
                        diagnostics.append(Diagnostic("error", "MD003", f"table row has {len(cells)} cells; expected {len(headers)}", line=line, column=1))
                    else:
                        rows.append(Row(tuple(cells), line))
                    row_index += 1
                result.append(Table(heading.group("title").strip(), first_line + index, tuple(headers), tuple(rows)))
                break
            cursor += 1
        index = end
    return result, diagnostics


def _normal(value: str) -> str:
    return re.sub(r"\s+", " ", re.sub(r"[*_`]", "", value)).strip().casefold()


def _number(value: str) -> Decimal | None:
    candidate = value.strip()
    if not candidate or candidate.casefold() == "auto":
        return None
    candidate = re.sub(r"(?:EUR|USD|GBP|INR|JPY|CAD|AUD|CHF|KZT)", "", candidate, flags=re.I)
    candidate = candidate.replace(",", "").translate(str.maketrans("", "", "€$£₹")).strip()
    negative = candidate.startswith("(") and candidate.endswith(")")
    if negative:
        candidate = candidate[1:-1]
    try:
        parsed = Decimal(candidate)
    except InvalidOperation:
        return None
    return -parsed if negative else parsed


def _frontmatter(path: Path, source: str) -> tuple[dict[str, Any] | None, str, int, list[Diagnostic]]:
    match = _FRONT.search(source)
    if not match:
        return None, source, 1, [Diagnostic("error", "YAML001", "invoice must begin with YAML frontmatter delimited by ---", str(path), 1, 1)]
    try:
        data = yaml.load(match.group("yaml"), Loader=UniqueKeyLoader) or {}
    except yaml.MarkedYAMLError as exc:
        mark = exc.problem_mark
        return None, source[match.end():], 1, [Diagnostic("error", "YAML002", exc.problem or str(exc), str(path), mark.line + 2 if mark else 1, mark.column + 1 if mark else 1)]
    if not isinstance(data, dict):
        return None, source[match.end():], 1, [Diagnostic("error", "YAML003", "frontmatter root must be a mapping", str(path), 2, 1)]
    return data, source[match.end():], source.count("\n", 0, match.end()) + 1, []


def _schema_checks(path: Path, data: dict[str, Any]) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    for key in ("schema", "invoice", "from", "to"):
        if key not in data:
            diagnostics.append(Diagnostic("error", "SCHEMA001", f"missing required frontmatter key {key!r}", str(path), 2, 1))
    if data.get("schema") != "ttyinv/v1":
        diagnostics.append(Diagnostic("error", "SCHEMA002", f"unsupported schema {data.get('schema')!r}; expected ttyinv/v1", str(path), 2, 1))
    invoice = data.get("invoice")
    if isinstance(invoice, dict):
        for key in ("number", "issued", "currency"):
            if key not in invoice:
                diagnostics.append(Diagnostic("error", "SCHEMA003", f"invoice.{key} is required", str(path), 2, 1))
        currency = invoice.get("currency")
        if currency is not None and not re.fullmatch(r"[A-Z]{3}", str(currency)):
            diagnostics.append(Diagnostic("error", "SCHEMA004", "invoice.currency must be a three-letter uppercase currency code", str(path), 2, 1))
    elif invoice is not None:
        diagnostics.append(Diagnostic("error", "SCHEMA005", "invoice must be a mapping", str(path), 2, 1))
    return diagnostics


def _money_checks(path: Path, data: dict[str, Any], tables: Iterable[Table], policy: str) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    currency = str((data.get("invoice") or {}).get("currency", "")).upper()
    financial = 0
    for table in tables:
        headers = [_normal(value) for value in table.headers]
        candidates = [(i, match.group(1).upper() if match.group(1) else None) for i, header in enumerate(headers) if (match := _AMOUNT.fullmatch(header))]
        amount_index = next((i for i, header_currency in candidates if header_currency == currency), candidates[-1][0] if candidates else None)
        if amount_index is None:
            continue
        financial += 1
        qty_index = next((i for i, header in enumerate(headers) if header in _QTY), None)
        rate_index = next((i for i, header in enumerate(headers) if header in _RATE), None)
        for row in table.rows:
            if row.cells and _normal(row.cells[0]) in {"total", "grand total"}:
                diagnostics.append(Diagnostic("warning", "MONEY001", "TOTAL rows are generated by ttyinv and should normally be omitted", str(path), row.line, 1))
            explicit = _number(row.cells[amount_index])
            calculated = None
            if qty_index is not None and rate_index is not None:
                quantity, rate = _number(row.cells[qty_index]), _number(row.cells[rate_index])
                if quantity is not None and rate is not None:
                    calculated = quantity * rate
            raw = row.cells[amount_index].strip()
            if not raw or raw.casefold() == "auto":
                if calculated is None:
                    diagnostics.append(Diagnostic("error", "MONEY002", "amount is auto/blank but quantity and rate are not both numeric", str(path), row.line, 1))
            elif explicit is None:
                diagnostics.append(Diagnostic("error", "MONEY003", f"could not parse amount {raw!r}", str(path), row.line, 1))
            elif calculated is not None and explicit.quantize(Decimal("0.01")) != calculated.quantize(Decimal("0.01")):
                if policy == "default":
                    diagnostics.append(Diagnostic("error", "MONEY004", f"explicit amount {explicit} differs from quantity × rate ({calculated})", str(path), row.line, 1, "Correct it, use auto, --trust-explicit, or --recalculate."))
                elif policy == "trust-explicit":
                    diagnostics.append(Diagnostic("info", "MONEY005", "explicit amount differs and will be trusted", str(path), row.line, 1))
        if len(table.rows) > 28:
            diagnostics.append(Diagnostic("warning", "PRINT001", f"section {table.title!r} has {len(table.rows)} rows and will likely span pages", str(path), table.heading_line, 1))
        for row in table.rows:
            if any(len(re.sub(r"<br\s*/?>", " ", cell, flags=re.I)) > 180 for cell in row.cells):
                diagnostics.append(Diagnostic("warning", "PRINT002", "a table cell exceeds 180 characters and may wrap heavily", str(path), row.line, 1))
    if not financial:
        diagnostics.append(Diagnostic("warning", "MONEY006", "no financial table with an Amount column was found", str(path)))
    return diagnostics


def lint_source(
    source_path: Path,
    *,
    source: str | None = None,
    allow_outside_root: bool = False,
    require_link_targets: bool = False,
    amount_policy: str = "default",
    theme: str = "light",
    paper: str | None = None,
    ink: str | None = None,
    muted: str | None = None,
    accent: str | None = None,
) -> list[Diagnostic]:
    text = source if source is not None else source_path.read_text(encoding="utf-8")
    data, body, first_line, diagnostics = _frontmatter(source_path, text)
    if data is not None:
        diagnostics.extend(_schema_checks(source_path, data))
        tables, table_diagnostics = parse_tables(body, first_line)
        diagnostics.extend(Diagnostic(item.severity, item.code, item.message, str(source_path), item.line, item.column, item.hint) for item in table_diagnostics)
        diagnostics.extend(_money_checks(source_path, data, tables, amount_policy))
    diagnostics.extend(validate_local_references(source_path, text, allow_outside_root=allow_outside_root, require_link_targets=require_link_targets))
    try:
        palette = resolve_palette(theme, paper=paper, ink=ink, muted=muted, accent=accent)
    except ValueError as exc:
        diagnostics.append(Diagnostic("error", "COLOR001", str(exc), str(source_path)))
    else:
        diagnostics.extend(contrast_diagnostics(palette, str(source_path)))
    return diagnostics


def diagnostics_json(diagnostics: Iterable[Diagnostic]) -> str:
    return json.dumps([item.as_dict() for item in diagnostics], indent=2, sort_keys=True)
