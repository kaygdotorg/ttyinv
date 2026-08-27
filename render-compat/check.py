#!/usr/bin/env python3
"""Check the render-compat corpus without depending on a renderer."""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
from decimal import Decimal, ROUND_HALF_EVEN
from pathlib import Path

ROOT = Path(__file__).resolve().parent
EXPONENTS = {
    "BHD": 3, "IQD": 3, "JOD": 3, "KWD": 3, "LYD": 3, "OMR": 3, "TND": 3,
    "BIF": 0, "CLP": 0, "DJF": 0, "GNF": 0, "ISK": 0, "JPY": 0,
    "KMF": 0, "KRW": 0, "PYG": 0, "RWF": 0, "UGX": 0, "UYI": 0,
    "VND": 0, "VUV": 0, "XAF": 0, "XOF": 0, "XPF": 0,
}
REQUIRED = {
    "fixture", "source", "valid", "config", "currency", "currency_exponent",
    "content_order", "sections", "fixed_blocks", "grand_total", "links",
    "images", "diagnostics", "method", "notes", "fixed_block_order",
}


def fail(message: str) -> None:
    raise ValueError(message)


def quantized(value: Decimal, exponent: int) -> Decimal:
    quantum = Decimal(1).scaleb(-exponent)
    return value.quantize(quantum, rounding=ROUND_HALF_EVEN)


def split_cell_row(line: str) -> list[str]:
    text = line.strip()
    if text.startswith("|"):
        text = text[1:]
    if text.endswith("|"):
        text = text[:-1]
    cells: list[str] = []
    cell: list[str] = []
    escaped = False
    for char in text:
        if escaped:
            cell.append(char)
            escaped = False
        elif char == "\\":
            escaped = True
        elif char == "|":
            cells.append("".join(cell).strip().replace("<br>", "\n"))
            cell = []
        else:
            cell.append(char)
    if escaped:
        cell.append("\\")
    cells.append("".join(cell).strip().replace("<br>", "\n"))
    return cells


def source_amounts(body_lines: list[str], currency: str) -> list[str]:
    table = [line for line in body_lines if line.startswith("|")]
    if len(table) < 3:
        return []
    headings = split_cell_row(table[0])
    rows = [split_cell_row(line) for line in table[2:]]
    normalized = ["".join(char.lower() for char in heading if char.isalnum()) for heading in headings]
    amount_candidates = [
        i for i, heading in enumerate(normalized)
        if heading.startswith("amount") or heading.startswith("total")
    ]
    payable = [
        i for i in amount_candidates
        if f"({currency.lower()})" in headings[i].lower()
    ]
    amount_column = (payable or amount_candidates)[-1]
    quantity = next(
        (i for i, heading in enumerate(normalized)
         if heading in {"qty", "quantity", "days", "hours", "units"}),
        None,
    )
    rate = next(
        (i for i, heading in enumerate(normalized)
         if heading in {"rate", "unitprice", "price"}),
        None,
    )
    exponent = EXPONENTS.get(currency, 2)
    output = []
    for row in rows:
        value = row[amount_column].strip()
        if value.lower() == "auto" or not value:
            if quantity is None or rate is None:
                fail("auto row lacks quantity or rate column")
            value = str(Decimal(row[quantity]) * Decimal(row[rate]))
        output.append(format(quantized(Decimal(value), exponent), f".{exponent}f"))
    return output



def rendered_amount(value: str, currency: str, source_text: str) -> str:
    format_name = next(
        (line.split(":", 1)[1].strip() for line in source_text.splitlines() if line.startswith("format:")),
        "code-comma-dot",
    )
    exponent = EXPONENTS.get(currency, 2)
    amount = Decimal(value).quantize(Decimal(1).scaleb(-exponent), rounding=ROUND_HALF_EVEN)
    rendered = format(amount, f",.{exponent}f")
    if format_name == "code-plain":
        return rendered.replace(",", "")
    if format_name == "code-dot-comma":
        return rendered.replace(",", "\x00").replace(".", ",").replace("\x00", ".")
    return rendered
def source_blocks(source: str) -> tuple[list[str], list[dict[str, object]], dict[str, bool], dict[str, bool]]:
    body = source.split("---\n", 2)[-1]
    lines = body.splitlines()
    title = next((line[2:].strip() for line in lines if line.startswith("# ")), None)
    if not title:
        fail("source has no H1 title")
    if not any(line.strip() == "- Number:" or line.startswith("- Number:") for line in lines):
        fail("source has no Number metadata")
    currency_line = next((line for line in lines if line.startswith("- Currency:")), "")
    currency = currency_line.split(":", 1)[1].strip()
    content = ["title", "metadata"]
    ordinary: list[dict[str, object]] = []
    fixed = {name: False for name in ("from", "bill_to", "settlements", "payment", "signature")}
    fixed_breaks = {name: False for name in fixed}
    pending = {"gap": "standard", "page_break_before": False, "summary_only": False}
    for index, line in enumerate(lines):
        if not line.startswith("## "):
            match = re.fullmatch(r"<!-- ttyinv:(.+) -->", line.strip())
            if match:
                directive = match.group(1)
                if directive == "page-break-before":
                    pending["page_break_before"] = True
                elif directive == "summary-only":
                    pending["summary_only"] = True
                elif directive.startswith("gap-before "):
                    pending["gap"] = directive[11:]
            continue
        heading = line[3:].strip()
        key = {"From": "from", "Bill to": "bill_to", "Settlements": "settlements", "Payment": "payment", "Signature": "signature"}.get(heading)
        if key:
            fixed[key] = True
            fixed_breaks[key] = bool(pending["page_break_before"])
            content.append(key)
            pending = {"gap": "standard", "page_break_before": False, "summary_only": False}
            continue
        end = next((j for j in range(index + 1, len(lines)) if lines[j].startswith("## ")), len(lines))
        body_lines = lines[index + 1:end]
        body_kind = "table" if any(line.startswith("|") for line in body_lines) else "prose"
        ordinary.append({"title": heading, "body": body_kind, "directives": dict(pending), "source_amounts": source_amounts(body_lines, currency) if body_kind == "table" else []})
        content.append("section:" + heading)
        pending = {"gap": "standard", "page_break_before": False, "summary_only": False}
    return content, ordinary, fixed, fixed_breaks


def run_render(command: list[str], source_path: Path, fmt: str) -> bytes | None:
    try:
        result = subprocess.run(
            command + [str(source_path), "--format", fmt, "--stdout"],
            cwd=ROOT.parent.parent,
            capture_output=True,
            check=False,
        )
    except OSError:
        return None
    if result.returncode != 0:
        fail(f"canonical CLI render failed for {source_path.name} ({fmt}): {result.stderr.decode(errors='replace').strip()}")
    payload = result.stdout
    signatures = {"html": b"<!doctype html>", "pdf": b"%PDF-", "png": b"\x89PNG\r\n\x1a\n"}
    if not payload.startswith(signatures[fmt]):
        fail(f"{source_path.name}: {fmt} output has an invalid signature")
    return payload

def png_dimensions(payload: bytes) -> tuple[int, int]:
    if len(payload) < 24 or payload[:8] != b"\x89PNG\r\n\x1a\n":
        fail("PNG signature or IHDR is missing")
    return int.from_bytes(payload[16:20], "big"), int.from_bytes(payload[20:24], "big")

def run_cli(command: list[str], source_path: Path) -> dict[str, object]:
    try:
        result = subprocess.run(
            command + [str(source_path), "--json"],
            cwd=ROOT.parent,
            capture_output=True,
            check=False,
        )
    except OSError as exc:
        fail(f"{source_path.name}: CLI failed to start: {exc}")
    if result.returncode != 0:
        fail(
            f"{source_path.name}: CLI returned {result.returncode}: "
            f"{result.stderr.decode(errors='replace').strip()}"
        )
    try:
        value = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        fail(f"{source_path.name}: CLI returned invalid JSON: {exc}")
    if not isinstance(value, dict):
        fail(f"{source_path.name}: CLI JSON response must be an object")
    return value



def cli_commands() -> tuple[list[str], list[str], list[str]] | None:
    binary = os.environ.get("TTYINV_BIN")
    if binary:
        return [binary, "validate"], [binary, "sections"], [binary, "render"]
    cargo = shutil.which("cargo")
    manifest = ROOT.parent / "Cargo.toml"
    if cargo and manifest.exists():
        prefix = [cargo, "run", "--quiet", "--manifest-path", str(manifest), "--bin", "ttyinv", "--"]
        return prefix + ["validate"], prefix + ["sections"], prefix + ["render"]
    return None


def check_contract(record: dict[str, object], source_path: Path) -> None:
    missing = REQUIRED - record.keys()
    if missing:
        fail(f"{record.get('fixture')}: missing keys {sorted(missing)}")
    currency = str(record["currency"])
    exponent = int(record["currency_exponent"])
    expected_exponent = EXPONENTS.get(currency, 2)
    if exponent != expected_exponent:
        fail(f"{record['fixture']}: exponent {exponent}, expected {expected_exponent}")
    source_content, source_sections, source_fixed, source_breaks = source_blocks(source_path.read_text(encoding="utf-8"))
    if record["content_order"] != source_content:
        fail(f"{record['fixture']}: content_order does not match source")
    expected_sections = record["sections"]
    if len(expected_sections) != len(source_sections):
        fail(f"{record['fixture']}: section count mismatch")
    for expected, actual in zip(expected_sections, source_sections):
        if expected["title"] != actual["title"] or expected["body"] != actual["body"] or expected["directives"] != actual["directives"]:
            fail(f"{record['fixture']}: section structure mismatch for {expected.get('title')}")
        if expected["row_amounts"] != actual["source_amounts"]:
            fail(f"{record['fixture']}: row amounts differ from source calculation for {expected['title']}")
        amounts = [Decimal(str(value)) for value in expected["row_amounts"]]
        total = Decimal(str(expected["total"]))
        if expected["directives"]["summary_only"]:
            if total != Decimal("0"):
                fail(f"{record['fixture']}: summary-only section must total zero")
        elif quantized(sum(amounts, Decimal("0")), exponent) != total:
            fail(f"{record['fixture']}: row amounts do not sum for {expected['title']}")
        if int(expected["currency_exponent"]) != exponent:
            fail(f"{record['fixture']}: section exponent mismatch")
    grand = sum((Decimal(str(section["total"])) for section in expected_sections if not section["directives"]["summary_only"]), Decimal("0"))
    if quantized(grand, exponent) != Decimal(str(record["grand_total"])):
        fail(f"{record['fixture']}: grand total mismatch")
    fixed = {str(item["name"]): item for item in record["fixed_blocks"]}
    if record["fixed_block_order"] != [str(item["name"]) for item in record["fixed_blocks"]]:
        fail(f"{record['fixture']}: fixed block order field mismatch")
    if set(fixed) != set(source_fixed):
        fail(f"{record['fixture']}: fixed block registry mismatch")
    for name, present in source_fixed.items():
        if bool(fixed[name]["present"]) != present:
            fail(f"{record['fixture']}: fixed block presence mismatch for {name}")
        if bool(fixed[name]["page_break_before"]) != source_breaks[name]:
            fail(f"{record['fixture']}: fixed block page-break mismatch for {name}")
    if not isinstance(record["diagnostics"], list):
        fail(f"{record['fixture']}: diagnostics must be a list")
    if not isinstance(record["links"], list) or not isinstance(record["images"], list):
        fail(f"{record['fixture']}: links and images must be lists")
    source_text = source_path.read_text(encoding="utf-8")
    source_links = re.findall(r"^- Website:\s*(https?://\S+)\s*$", source_text, re.MULTILINE)
    source_links.extend(re.findall(r"(?<!!)\[[^\]]+\]\((https?://[^)]+)\)", source_text))
    if sorted(str(value) for value in record["links"]) != sorted(source_links):
        fail(f"{record['fixture']}: link manifest does not match authored links")
    source_images = [
        {"alt": alt, "src": src}
        for alt, src in re.findall(r"!\[([^\]]+)\]\(([^)]+)\)", source_text)
    ]
    manifest_images = [{"alt": str(item["alt"]), "src": str(item["src"])} for item in record["images"]]
    if manifest_images != source_images:
        fail(f"{record['fixture']}: image manifest does not match authored images")


def main() -> int:
    index_path = ROOT / "index.json"
    index = json.loads(index_path.read_text(encoding="utf-8"))
    entries = index.get("fixtures")
    if not isinstance(entries, list) or not entries:
        fail("index has no fixtures")
    ids = [entry["id"] for entry in entries]
    if len(ids) != len(set(ids)):
        fail("index contains duplicate fixture ids")
    listed = {f"{entry['id']}.{suffix}" for entry in entries for suffix in ("md", "json")}
    actual = {path.name for path in ROOT.iterdir() if path.name not in {"index.json", Path(__file__).name}}
    if actual != listed:
        fail(f"index/file mismatch; unlisted files: {sorted(actual - listed)}; missing files: {sorted(listed - actual)}")
    commands = cli_commands()
    cli_checked = 0
    for entry in entries:
        source_path = ROOT / entry["source"]
        expected_path = ROOT / entry["expected"]
        record = json.loads(expected_path.read_text(encoding="utf-8"))
        if record["fixture"] != entry["id"] or record["source"] != entry["source"]:
            fail(f"{entry['id']}: index and contract identity mismatch")
        check_contract(record, source_path)
        if commands:
            validate = run_cli(commands[0], source_path)
            sections = run_cli(commands[1], source_path)
            if validate.get("valid") != record["valid"] or validate.get("diagnostics") != record["diagnostics"]:
                fail(f"{entry['id']}: validate output differs from expected validity or diagnostics")
            if not isinstance(validate.get("revision"), str) or not validate["revision"]:
                fail(f"{entry['id']}: validate output is missing canonical revision")
            expected_manifest = [{"index": i, "title": s["title"], "body": s["body"], "gap": s["directives"]["gap"], "page_break_before": s["directives"]["page_break_before"], "summary_only": s["directives"]["summary_only"]} for i, s in enumerate(record["sections"])]
            if sections["ordinary_sections"] != expected_manifest:
                fail(f"{entry['id']}: sections output differs from expected directives")
            for fmt in ("html", "pdf", "png"):
                payload = run_render(commands[2], source_path, fmt)
                if payload is None:
                    fail(f"{entry['id']}: renderer unavailable for {fmt}")
                repeat = run_render(commands[2], source_path, fmt)
                if repeat != payload:
                    fail(f"{entry['id']}: {fmt} output is not deterministic")
                page_breaks = sum(
                    bool(item["page_break_before"])
                    for item in record["fixed_blocks"]
                ) + sum(bool(item["directives"]["page_break_before"]) for item in record["sections"])
                source_text = source_path.read_text(encoding="utf-8")
                if fmt == "html":
                    html = payload.decode("utf-8")
                    if 'class="page' not in html:
                        fail(f"{entry['id']}: HTML has no page article")
                    title = next(line[2:].strip() for line in source_text.splitlines() if line.startswith("# "))
                    if Decimal(str(record["grand_total"])) != Decimal("0"):
                        expected_total = rendered_amount(record["grand_total"], record["currency"], source_text)
                        expected_total = f"{record['currency']}\u00a0{expected_total}"
                        if f"<strong>Total due</strong> {expected_total}" not in html:
                            fail(f"{entry['id']}: HTML total content mismatch")
                    if "**" in source_text and "<strong>" not in html:
                        fail(f"{entry['id']}: strong Markdown semantics missing")
                    if re.search(r"(?<!\\)\*[^*]+\*", source_text) and "<em>" not in html:
                        fail(f"{entry['id']}: emphasis Markdown semantics missing")
                    if ":---:" in source_text and 'class="center"' not in html:
                        fail(f"{entry['id']}: centered table alignment missing")
                elif fmt == "pdf":
                    pages = payload.count(b"/Type /Page") + payload.count(b"/Type/Page")
                    if pages < 1 or pages <= page_breaks:
                        fail(f"{entry['id']}: PDF pagination does not reflect directives")
                elif fmt == "png":
                    width, height = png_dimensions(payload)
                    if width != 595 or height < 842 or height % 842:
                        fail(f"{entry['id']}: PNG geometry is not A4 page-stacked")
            cli_checked += 1
    if commands:
        print(f"checked {len(entries)} fixtures and canonical CLI output ({cli_checked} fixtures)")
    else:
        print(f"checked {len(entries)} fixtures; canonical CLI unavailable, source invariants only")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"render-compat check failed: {error}", file=sys.stderr)
        raise SystemExit(1)
