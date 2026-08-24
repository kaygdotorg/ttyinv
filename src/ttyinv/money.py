from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from decimal import Decimal, InvalidOperation

from babel.numbers import format_currency, format_decimal, get_currency_symbol

from .errors import TtyinvError
from .models import (
    AmountPolicy,
    CalculatedCell,
    CalculatedFinancialSection,
    CalculatedInvoice,
    CalculatedProseSection,
    CalculatedRow,
    FinancialSection,
    ParsedInvoice,
)

_QUANTITY_ALIASES = {"qty", "quantity", "days", "hours", "units"}
_RATE_ALIASES = {"rate", "unitprice", "price"}
_DESCRIPTION_ALIASES = {"description", "item", "service"}
_CURRENCY_RE = re.compile(r"\(([A-Za-z]{3})\)")
_SUMMARY_LABELS = {"subtotal", "total", "grand total"}
RATE_ROUNDING_WARNING_CODE = "MONEY007"


@dataclass(slots=True)
class ColumnInfo:
    description: int | None
    quantity: int | None
    rate: int | None
    payable_amount: int
    currencies: dict[int, str]


def _normalise_header(value: str) -> str:
    return re.sub(r"[^a-z0-9]", "", value.lower())


def _header_currency(value: str) -> str | None:
    match = _CURRENCY_RE.search(value)
    return match.group(1).upper() if match else None


def _inspect_columns(section: FinancialSection, invoice_currency: str) -> ColumnInfo:
    description: int | None = None
    quantity: int | None = None
    rate: int | None = None
    amount_candidates: list[int] = []
    currencies: dict[int, str] = {}

    for index, header in enumerate(section.table.headers):
        normalised = _normalise_header(header.source)
        currency = _header_currency(header.source)
        if currency:
            currencies[index] = currency
        if normalised in _DESCRIPTION_ALIASES:
            description = index
        if normalised in _QUANTITY_ALIASES:
            quantity = index
        if normalised in _RATE_ALIASES:
            rate = index
        if normalised == "amount" or re.fullmatch(r"amount[a-z]{3}", normalised) or normalised == "total" or re.fullmatch(r"total[a-z]{3}", normalised):
            amount_candidates.append(index)
    matching = [index for index in amount_candidates if currencies.get(index) == invoice_currency]
    if len(matching) > 1:
        raise TtyinvError(
            f"Section {section.title!r} must have exactly one payable Amount column. "
            f"Use 'Amount' or 'Amount ({invoice_currency})'."
        )
    if not amount_candidates:
        raise TtyinvError(
            f"Section {section.title!r} must have exactly one payable Amount column. "
            f"Use 'Amount' or 'Amount ({invoice_currency})'."
        )

    return ColumnInfo(description, quantity, rate, matching[0] if matching else amount_candidates[-1], currencies)


def _parse_decimal(value: str, context: str, *, source_path: Path | None = None, source_line: int | None = None) -> Decimal | None:
    trimmed = value.strip()
    if not trimmed or trimmed.lower() == "auto":
        return None
    normalised = re.sub(r"[\s,_]", "", trimmed)
    normalised = re.sub(r"^[^0-9+\-.]+", "", normalised)
    normalised = re.sub(r"[^0-9+\-.]+$", "", normalised)
    try:
        return Decimal(normalised)
    except InvalidOperation as exc:
        raise TtyinvError(f"Invalid numeric value {value!r} in {context}.", path=source_path, line=source_line) from exc


def _babel_locale(locale: str) -> str:
    return locale.replace("-", "_")


def display_money(amount: Decimal, currency: str, locale: str) -> str:
    try:
        babel_locale = _babel_locale(locale)
        formatted = format_currency(amount, currency, locale=babel_locale, currency_digits=True, decimal_quantization=False)
        symbol = get_currency_symbol(currency, locale=babel_locale)
        if not symbol or symbol not in formatted:
            return formatted
        marker_index = formatted.index(symbol)
        before = formatted[:marker_index].rstrip()
        after = formatted[marker_index + len(symbol):].lstrip()
        # Preserve the locale's symbol position while guaranteeing one
        # nonbreaking visual gap between the marker and the amount.
        return f"{before}{symbol}\u00a0{after}" if after else f"{before}\u00a0{symbol}"
    except Exception:
        return f"{currency}\u00a0{amount.quantize(Decimal('0.01'))}"


_ONES = ["Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten", "Eleven", "Twelve", "Thirteen", "Fourteen", "Fifteen", "Sixteen", "Seventeen", "Eighteen", "Nineteen"]
_TENS = ["", "", "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety"]


def _under_thousand(value: int) -> str:
    parts: list[str] = []
    if value >= 100:
        parts.extend((_ONES[value // 100], "Hundred")); value %= 100
    if value >= 20:
        tens = _TENS[value // 10]; parts.append(f"{tens}-{_ONES[value % 10]}" if value % 10 else tens)
    elif value:
        parts.append(_ONES[value])
    return " ".join(parts)


def _western_integer_words(value: int) -> str:
    if value == 0: return "Zero"
    if value < 0: return "Minus " + _western_integer_words(-value)
    parts: list[str] = []
    for scale, label in ((1_000_000_000, "Billion"), (1_000_000, "Million"), (1_000, "Thousand")):
        if value >= scale:
            count, value = divmod(value, scale); parts.extend((_western_integer_words(count), label))
    if value: parts.append(_under_thousand(value))
    return " ".join(part for part in parts if part)


def _indian_integer_words(value: int) -> str:
    if value == 0: return "Zero"
    if value < 0: return "Minus " + _indian_integer_words(-value)
    parts: list[str] = []
    for scale, label in ((10_000_000, "Crore"), (100_000, "Lakh"), (1_000, "Thousand")):
        if value >= scale:
            count, value = divmod(value, scale); parts.extend((_western_integer_words(count), label))
    if value: parts.append(_under_thousand(value))
    return " ".join(part for part in parts if part)


def money_in_words(amount: Decimal, currency: str) -> str:
    currency = currency.upper(); quantized = amount.quantize(Decimal("0.01")); sign = -1 if quantized < 0 else 1
    absolute = abs(quantized); major = int(absolute); minor = int((absolute - Decimal(major)) * 100)
    if currency == "EUR":
        parts = [_western_integer_words(major), "Euro" if major == 1 else "Euros"]
        if minor: parts.extend(("and", _western_integer_words(minor), "Cent" if minor == 1 else "Cents"))
    elif currency == "INR":
        parts = [_indian_integer_words(major), "Rupee" if major == 1 else "Rupees"]
        if minor: parts.extend(("and", _western_integer_words(minor), "Paisa" if minor == 1 else "Paise"))
    else:
        parts = [_western_integer_words(major), currency]
        if minor: parts.extend(("and", _western_integer_words(minor), "Hundredths"))
    result = " ".join(parts) + " Only"
    return "Minus " + result if sign < 0 else result


def display_number(amount: Decimal, locale: str) -> str:
    try: return format_decimal(amount, locale=_babel_locale(locale), decimal_quantization=False)
    except Exception: return format(amount, "f")


def within_authored_rate_rounding(explicit: Decimal, calculated: Decimal, quantity: Decimal, rate: Decimal) -> bool:
    """Accept only discrepancies explainable by the authored rate precision.

    A displayed rate rounded to ``scale`` decimal places may differ from its
    underlying value by at most half a unit at that scale. The explicit
    currency amount is itself rounded to cents, so one half cent is included
    for that independent rounding. This keeps the tolerance proportional to
    the authored quantity and rate precision instead of using a blanket
    amount threshold.
    """

    rate_scale = max(0, -rate.as_tuple().exponent)
    rate_error = abs(quantity) * (Decimal("0.5") * (Decimal(10) ** -rate_scale))
    amount_error = Decimal("0.005")
    return abs(explicit - calculated) <= rate_error + amount_error


def _summary_label(value: str) -> str | None:
    normalized = re.sub(r"[*_`]", "", value).strip().casefold()
    return normalized if normalized in _SUMMARY_LABELS else None


def _calculate_section(section: FinancialSection, invoice_currency: str, locale: str, policy: AmountPolicy, source_path: Path) -> CalculatedFinancialSection:
    columns = _inspect_columns(section, invoice_currency); total = Decimal("0"); rows: list[CalculatedRow] = []; warnings: list[str] = []
    for row_index, row in enumerate(section.table.rows, start=1):
        source_line = section.table.row_lines[row_index - 1] if row_index - 1 < len(section.table.row_lines) else section.line
        description = row[columns.description].source if columns.description is not None else row[0].source
        summary_label = _summary_label(description)
        quantity = None if summary_label else (_parse_decimal(row[columns.quantity].source, f"{section.title}, row {row_index}, quantity", source_path=source_path, source_line=source_line) if columns.quantity is not None else None)
        rate = None if summary_label else (_parse_decimal(row[columns.rate].source, f"{section.title}, row {row_index}, rate", source_path=source_path, source_line=source_line) if columns.rate is not None else None)
        explicit = _parse_decimal(row[columns.payable_amount].source, f"{section.title}, row {row_index}, amount", source_path=source_path, source_line=source_line)
        computed = quantity * rate if quantity is not None and rate is not None else None
        if summary_label:
            if explicit is None:
                raise TtyinvError(f"{section.title}, row {row_index}: authored {summary_label} row must include an explicit payable amount.", path=source_path, line=source_line, code="MONEY008")
            amount, amount_source = explicit, "authored-summary"
        elif policy.recalculate and computed is not None: amount, amount_source = computed, "calculated"
        elif explicit is None and computed is not None: amount, amount_source = computed, "calculated"
        elif explicit is not None and computed is not None:
            if abs(explicit - computed) <= Decimal("0.005"): amount, amount_source = explicit, "explicit"
            elif within_authored_rate_rounding(explicit, computed, quantity, rate):
                amount, amount_source = explicit, "explicit-rounded-rate"
                warnings.append(f"{RATE_ROUNDING_WARNING_CODE}: explicit amount accepted because the authored rate is rounded to displayed precision.")
            elif policy.trust_explicit: amount, amount_source = explicit, "trusted-explicit"
            else: raise TtyinvError(f"{section.title}, row {row_index}: explicit amount {explicit} does not match quantity x rate ({computed}). Use --trust-explicit to keep the written amount or --recalculate to replace it.", path=source_path, line=source_line, code="MONEY004")
        elif explicit is not None: amount, amount_source = explicit, "explicit"
        else: raise TtyinvError(f"{section.title}, row {row_index}: Amount is blank, but no calculable quantity and rate were found.", path=source_path, line=source_line)
        if not section.summary_only and not summary_label:
            total += amount
        cells: list[CalculatedCell] = []
        for cell_index, cell in enumerate(row):
            if cell_index == columns.payable_amount:
                rendered = display_money(amount, invoice_currency, locale); cells.append(CalculatedCell(rendered, rendered, True)); continue
            if cell_index == columns.quantity and quantity is not None:
                rendered = display_number(quantity, locale); cells.append(CalculatedCell(rendered, rendered, True)); continue
            if cell_index == columns.rate and rate is not None:
                rendered = display_money(rate, invoice_currency, locale); cells.append(CalculatedCell(rendered, rendered, True)); continue
            column_currency = columns.currencies.get(cell_index)
            if column_currency:
                numeric = _parse_decimal(cell.source, f"{section.title}, row {row_index}, {section.table.headers[cell_index].source}", source_path=source_path, source_line=source_line)
                if numeric is not None:
                    rendered = display_money(numeric, column_currency, locale); cells.append(CalculatedCell(rendered, rendered, True)); continue
            cells.append(CalculatedCell(cell.html, cell.source, False))
        rows.append(CalculatedRow(cells, amount, amount_source, source_line, summary_label))
    return CalculatedFinancialSection(section.title, [h.html for h in section.table.headers], section.table.align, rows, total, columns.payable_amount, section.line, warnings, section.page_break_before, section.summary_only)


def calculate_invoice(invoice: ParsedInvoice, policy: AmountPolicy) -> CalculatedInvoice:
    if policy.trust_explicit and policy.recalculate: raise TtyinvError("--trust-explicit and --recalculate cannot be used together.")
    currency = invoice.frontmatter.invoice.currency; locale = invoice.frontmatter.invoice.locale; sections = []; grand_total = Decimal("0"); warnings: list[str] = []
    for section in invoice.sections:
        if section.kind == "prose":
            sections.append(CalculatedProseSection(section.title, section.html, section.line, section.page_break_before, section.summary_only)); continue
        calculated = _calculate_section(section, currency, locale, policy, invoice.source_path)
        calculated.page_break_before = section.page_break_before
        sections.append(calculated); grand_total += calculated.total; warnings.extend(calculated.warnings)
    return CalculatedInvoice(invoice.source_path, invoice.source_directory, invoice.frontmatter, invoice.preamble_html, sections, grand_total, warnings)
