"""Source-aware validation used by ``ttyinv lint`` and pre-render checks."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, replace
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Any, Iterable

import yaml
from pydantic import ValidationError
from yaml.constructor import ConstructorError

from .appearance import contrast_diagnostics, resolve_palette
from .diagnostics import Diagnostic
from .models import InvoiceFrontmatter
from .money import RATE_ROUNDING_WARNING_CODE, within_authored_rate_rounding
from .security import validate_local_references
from .yaml_support import MAX_YAML_DEPTH, StringDateSafeLoader

_FRONT = re.compile(r"\A---[ \t]*\r?\n(?P<yaml>.*?)\r?\n---[ \t]*(?:\r?\n|\Z)", re.DOTALL)
_H2 = re.compile(r"^##[ \t]+(?P<title>.+?)[ \t]*$")
_SEPARATOR = re.compile(r"^:?-{3,}:?$")
_AMOUNT = re.compile(r"^(?:amount|total)(?:\s*\(([A-Z]{3})\))?$", re.I)
_QTY = {"qty", "quantity", "days", "hours", "units"}
_RATE = {"rate", "unit price", "unit_price", "price"}
_DESCRIPTION = {"description", "item", "service"}


class UniqueKeyLoader(StringDateSafeLoader):
    pass


class _YamlTraversalError(Exception):
    """Report a YAML alias cycle or excessive nesting."""


def _construct_mapping(loader: UniqueKeyLoader, node: yaml.MappingNode, deep: bool = False) -> dict[Any, Any]:
    mapping: dict[Any, Any] = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        try:
            duplicate = key in mapping
        except TypeError as exc:
            raise ConstructorError("while constructing a mapping", node.start_mark, "mapping keys must be hashable", key_node.start_mark) from exc
        if duplicate:
            raise ConstructorError("while constructing a mapping", node.start_mark, "found duplicate key", key_node.start_mark)
        try:
            mapping[key] = loader.construct_object(value_node, deep=deep)
        except TypeError as exc:
            raise ConstructorError("while constructing a mapping", node.start_mark, "mapping keys must be hashable", key_node.start_mark) from exc
    return mapping


def _safe_schema_message(error: dict[str, Any]) -> str:
    message = str(error.get("msg", ""))
    if "real calendar date" in message:
        return "date must be a real calendar date"
    if "YYYY-MM-DD" in message:
        return "date must use YYYY-MM-DD"
    if "due date must be on or after issue date" in message:
        return "due date must be on or after issue date"
    return "invalid value"


UniqueKeyLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _construct_mapping)


@dataclass(frozen=True, slots=True)
class Row:
    cells: tuple[str, ...]
    line: int
    columns: tuple[int, ...]


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


def _split_with_columns(line: str) -> tuple[list[str], list[int]]:
    """Split a table row and retain each cell's one-based source column."""

    start = len(line) - len(line.lstrip())
    end = len(line.rstrip())
    if start < end and line[start] == "|":
        start += 1
    if start < end and line[end - 1] == "|" and (end < 2 or line[end - 2] != "\\"):
        end -= 1
    cells: list[str] = []
    columns: list[int] = []
    cell_start = start
    escaped = False
    for index in range(start, end):
        char = line[index]
        if escaped:
            escaped = False
        elif char == "\\":
            escaped = True
        elif char == "|":
            raw = line[cell_start:index]
            cells.append(raw.strip())
            columns.append(cell_start + len(raw) - len(raw.lstrip()) + 1)
            cell_start = index + 1
    raw = line[cell_start:end]
    cells.append(raw.strip())
    columns.append(cell_start + len(raw) - len(raw.lstrip()) + 1)
    return cells, columns


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
                    cells, columns = _split_with_columns(lines[row_index])
                    line = first_line + row_index
                    if len(cells) != len(headers):
                        diagnostics.append(Diagnostic("error", "MD003", f"table row has {len(cells)} cells; expected {len(headers)}", line=line, column=1, section=heading.group("title").strip(), row=row_index - cursor - 1))
                    else:
                        rows.append(Row(tuple(cells), line, tuple(columns)))
                    row_index += 1
                result.append(Table(heading.group("title").strip(), first_line + index, tuple(headers), tuple(rows)))
                break
            separator_cells = _split(lines[cursor + 1]) if "|" in lines[cursor + 1] else []
            separator_like = separator_cells and all(
                "-" in cell and re.fullmatch(r":?[- ]+:?", cell) for cell in separator_cells
            )
            if len(headers) >= 2 and separator_like:
                diagnostics.append(Diagnostic(
                    "error", "MD002",
                    f"malformed table separator; expected {len(headers)} cells containing at least three dashes",
                    line=first_line + cursor + 1, column=1, section=heading.group("title").strip(),
                    hint="Use a separator such as | --- | ---: | directly below the heading row.",
                ))
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
    except RecursionError:
        return None, source[match.end():], 1, [Diagnostic("error", "YAML002", "YAML nesting exceeds the supported limit", str(path), 2, 1)]
    except yaml.MarkedYAMLError as exc:
        mark = exc.problem_mark
        message = "mapping keys must be hashable" if exc.problem == "mapping keys must be hashable" else "invalid YAML frontmatter"
        return None, source[match.end():], 1, [Diagnostic("error", "YAML002", message, str(path), mark.line + 2 if mark else 1, mark.column + 1 if mark else 1)]
    if not isinstance(data, dict):
        return None, source[match.end():], 1, [Diagnostic("error", "YAML003", "frontmatter root must be a mapping", str(path), 2, 1)]
    return data, source[match.end():], source.count("\n", 0, match.end()) + 1, []


YamlPath = tuple[str | int, ...]


def _yaml_locations(yaml_source: str) -> dict[YamlPath, tuple[int, int]]:
    """Map YAML object paths while rejecting cycles and deep nesting."""
    try:
        root = yaml.compose(yaml_source, Loader=UniqueKeyLoader)
    except RecursionError as exc:
        raise _YamlTraversalError from exc
    locations: dict[YamlPath, tuple[int, int]] = {}
    active: set[int] = set()

    def visit(node: yaml.Node, path: YamlPath, depth: int) -> None:
        identity = id(node)
        if depth > MAX_YAML_DEPTH or identity in active:
            raise _YamlTraversalError
        active.add(identity)
        try:
            locations.setdefault(path, (node.start_mark.line + 2, node.start_mark.column + 1))
            if isinstance(node, yaml.MappingNode):
                for key_node, value_node in node.value:
                    key = str(key_node.value)
                    child = (*path, key)
                    locations[child] = (key_node.start_mark.line + 2, key_node.start_mark.column + 1)
                    visit(value_node, child, depth + 1)
            elif isinstance(node, yaml.SequenceNode):
                for index, child_node in enumerate(node.value):
                    child = (*path, index)
                    locations[child] = (child_node.start_mark.line + 2, child_node.start_mark.column + 1)
                    visit(child_node, child, depth + 1)
        finally:
            active.remove(identity)

    if root is not None:
        visit(root, (), 0)
    return locations


def _diagnostic_location(locations: dict[YamlPath, tuple[int, int]], path: YamlPath) -> tuple[int, int]:
    candidate = path
    while candidate:
        if candidate in locations:
            return locations[candidate]
        candidate = candidate[:-1]
    return locations.get((), (2, 1))


def _schema_checks(path: Path, data: dict[str, Any], locations: dict[YamlPath, tuple[int, int]]) -> list[Diagnostic]:
    try:
        InvoiceFrontmatter.model_validate(data)
    except ValidationError as exc:
        diagnostics: list[Diagnostic] = []
        for error in exc.errors(include_url=False):
            location = tuple(error["loc"])
            dotted = ".".join(str(part) for part in location)
            line, column = _diagnostic_location(locations, location)
            error_type = str(error["type"])
            if error_type == "extra_forbidden":
                code = "SCHEMA006"
                message = f"unknown frontmatter field {dotted!r}"
            elif error_type == "missing" and len(location) == 1:
                code = "SCHEMA001"
                message = f"missing required frontmatter key {dotted!r}"
            elif error_type == "missing" and location[:1] == ("invoice",):
                code = "SCHEMA003"
                message = f"{dotted} is required"
            elif location == ("schema",):
                code = "SCHEMA002"
                message = "unsupported schema; expected ttyinv/v1"
            else:
                code = "SCHEMA005"
                message = f"invalid {dotted or 'frontmatter'}: {_safe_schema_message(error)}"
            diagnostics.append(Diagnostic("error", code, message, str(path), line, column))
        return diagnostics
    return []


def _money_checks(path: Path, data: dict[str, Any], tables: Iterable[Table], policy: str) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    currency = str((data.get("invoice") or {}).get("currency", "")).upper()
    financial = 0
    for table in tables:
        headers = [_normal(value) for value in table.headers]
        candidates = [(i, match.group(1).upper() if match.group(1) else None) for i, header in enumerate(headers) if (match := _AMOUNT.fullmatch(header))]
        matching = [i for i, header_currency in candidates if header_currency == currency]
        if matching:
            amount_index = matching[0] if len(matching) == 1 else None
        else:
            amount_index = candidates[-1][0] if candidates else None
        if candidates and amount_index is None:
            diagnostics.append(Diagnostic("error", "MONEY001", f"table {table.title!r} must have exactly one payable amount column matching invoice currency {currency}", str(path), table.heading_line, 1, section=table.title))
            continue
        if amount_index is None:
            continue
        financial += 1
        qty_index = next((i for i, header in enumerate(headers) if header in _QTY), None)
        rate_index = next((i for i, header in enumerate(headers) if header in _RATE), None)
        description_index = next((i for i, header in enumerate(headers) if header in _DESCRIPTION), 0)
        for row_number, row in enumerate(table.rows, start=1):
            summary_label = _normal(row.cells[description_index]) if row.cells else ""
            if summary_label in {"subtotal", "total", "grand total"}:
                diagnostics.append(Diagnostic("warning", "MONEY001", "TOTAL rows are generated by ttyinv and should normally be omitted", str(path), row.line, row.columns[description_index], section=table.title, row=row_number, column_name=table.headers[description_index]))
            explicit = _number(row.cells[amount_index])
            calculated = None
            if summary_label not in {"subtotal", "total", "grand total"} and qty_index is not None and rate_index is not None:
                quantity, rate = _number(row.cells[qty_index]), _number(row.cells[rate_index])
                if quantity is not None and rate is not None:
                    calculated = quantity * rate
            raw = row.cells[amount_index].strip()
            if summary_label in {"subtotal", "total", "grand total"}:
                if not raw or raw.casefold() == "auto" or explicit is None:
                    diagnostics.append(Diagnostic("error", "MONEY008", "authored subtotal or total rows require an explicit numeric payable amount", str(path), row.line, row.columns[amount_index], section=table.title, row=row_number, column_name=table.headers[amount_index]))
                continue
            if not raw or raw.casefold() == "auto":
                if calculated is None:
                    diagnostics.append(Diagnostic("error", "MONEY002", "amount is auto/blank but quantity and rate are not both numeric", str(path), row.line, row.columns[amount_index], section=table.title, row=row_number, column_name=table.headers[amount_index]))
            elif explicit is None:
                diagnostics.append(Diagnostic("error", "MONEY003", f"could not parse amount {raw!r}", str(path), row.line, row.columns[amount_index], section=table.title, row=row_number, column_name=table.headers[amount_index]))
            elif calculated is not None and explicit.quantize(Decimal("0.01")) != calculated.quantize(Decimal("0.01")):
                if quantity is not None and rate is not None and within_authored_rate_rounding(explicit, calculated, quantity, rate):
                    diagnostics.append(Diagnostic("warning", RATE_ROUNDING_WARNING_CODE, "explicit amount accepted because the authored rate is rounded to displayed precision", str(path), row.line, row.columns[amount_index], section=table.title, row=row_number, column_name=table.headers[amount_index]))
                elif policy == "default":
                    diagnostics.append(Diagnostic("error", "MONEY004", f"explicit amount {explicit} differs from quantity × rate ({calculated})", str(path), row.line, row.columns[amount_index], "Correct it, use auto, --trust-explicit, or --recalculate.", table.title, row_number, table.headers[amount_index]))
                elif policy == "trust-explicit":
                    diagnostics.append(Diagnostic("info", "MONEY005", "explicit amount differs and will be trusted", str(path), row.line, row.columns[amount_index], section=table.title, row=row_number, column_name=table.headers[amount_index]))
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
    tables: list[Table] = []
    if data is not None:
        match = _FRONT.search(text)
        yaml_structure_error = False
        try:
            locations = _yaml_locations(match.group("yaml")) if match else {}
        except _YamlTraversalError:
            locations = {}
            yaml_structure_error = True
            diagnostics.append(Diagnostic("error", "YAML002", "YAML nesting or aliases exceed the supported limit", str(source_path), 2, 1))
        schema_diagnostics = [] if yaml_structure_error else _schema_checks(source_path, data, locations)
        diagnostics.extend(schema_diagnostics)
        tables, table_diagnostics = parse_tables(body, first_line)
        diagnostics.extend(replace(item, path=str(source_path)) for item in table_diagnostics)
        if not schema_diagnostics and not yaml_structure_error:
            diagnostics.extend(_money_checks(source_path, data, tables, amount_policy))
    reference_diagnostics = validate_local_references(source_path, text, allow_outside_root=allow_outside_root, require_link_targets=require_link_targets)
    heading_lines = [
        (text.count("\n", 0, match.start()) + 1, match.group("title").strip())
        for match in re.finditer(r"^##[ \t]+(?P<title>.+?)[ \t]*$", text, re.MULTILINE)
    ]
    for diagnostic in reference_diagnostics:
        section = next((title for line, title in reversed(heading_lines) if diagnostic.line is not None and line <= diagnostic.line), None)
        diagnostics.append(replace(diagnostic, section=section) if section else diagnostic)
    try:
        palette = resolve_palette(theme, paper=paper, ink=ink, muted=muted, accent=accent)
    except ValueError as exc:
        diagnostics.append(Diagnostic("error", "COLOR001", str(exc), str(source_path)))
    else:
        diagnostics.extend(contrast_diagnostics(palette, str(source_path)))
    return diagnostics


def diagnostics_json(diagnostics: Iterable[Diagnostic]) -> str:
    return json.dumps([item.as_dict() for item in diagnostics], indent=2, sort_keys=True)
