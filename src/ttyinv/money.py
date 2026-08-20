from __future__ import annotations

import re
from dataclasses import dataclass
from decimal import Decimal, InvalidOperation

from babel.numbers import format_currency, format_decimal

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
    payable_candidates: list[int] = []
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
        if normalised.startswith("amount") or normalised == "total":
            amount_candidates.append(index)
            if currency is None or currency == invoice_currency:
                payable_candidates.append(index)

    if not payable_candidates and len(amount_candidates) == 1:
        payable_candidates = amount_candidates.copy()
    if len(payable_candidates) != 1:
        raise TtyinvError(
            f"Section {section.title!r} must have exactly one payable Amount column. "
            f"Use 'Amount' or 'Amount ({invoice_currency})'."
        )

    return ColumnInfo(
        description=description,
        quantity=quantity,
        rate=rate,
        payable_amount=payable_candidates[0],
        currencies=currencies,
    )


def _parse_decimal(value: str, context: str) -> Decimal | None:
    trimmed = value.strip()
    if not trimmed or trimmed.lower() == "auto":
        return None
    normalised = re.sub(r"[\s,_]", "", trimmed)
    normalised = re.sub(r"^[^0-9+\-.]+", "", normalised)
    normalised = re.sub(r"[^0-9+\-.]+$", "", normalised)
    try:
        return Decimal(normalised)
    except InvalidOperation as exc:
        raise TtyinvError(f"Invalid numeric value {value!r} in {context}.") from exc


def _babel_locale(locale: str) -> str:
    return locale.replace("-", "_")


def display_money(amount: Decimal, currency: str, locale: str) -> str:
    try:
        return format_currency(
            amount,
            currency,
            locale=_babel_locale(locale),
            currency_digits=True,
            decimal_quantization=False,
        )
    except Exception:
        return f"{currency} {amount.quantize(Decimal('0.01'))}"


def display_number(amount: Decimal, locale: str) -> str:
    try:
        return format_decimal(amount, locale=_babel_locale(locale), decimal_quantization=False)
    except Exception:
        return format(amount, "f")


def _calculate_section(
    section: FinancialSection,
    invoice_currency: str,
    locale: str,
    policy: AmountPolicy,
) -> CalculatedFinancialSection:
    columns = _inspect_columns(section, invoice_currency)
    total = Decimal("0")
    rows: list[CalculatedRow] = []

    for row_index, row in enumerate(section.table.rows, start=1):
        description = row[columns.description].source if columns.description is not None else ""
        if description.strip().lower() == "total":
            raise TtyinvError(
                f"Section {section.title!r}, row {row_index}: do not write a TOTAL row; ttyinv generates it."
            )

        quantity = (
            _parse_decimal(row[columns.quantity].source, f"{section.title}, row {row_index}, quantity")
            if columns.quantity is not None
            else None
        )
        rate = (
            _parse_decimal(row[columns.rate].source, f"{section.title}, row {row_index}, rate")
            if columns.rate is not None
            else None
        )
        explicit = _parse_decimal(
            row[columns.payable_amount].source,
            f"{section.title}, row {row_index}, amount",
        )
        computed = quantity * rate if quantity is not None and rate is not None else None

        if policy.recalculate and computed is not None:
            amount = computed
            amount_source = "calculated"
        elif explicit is None and computed is not None:
            amount = computed
            amount_source = "calculated"
        elif explicit is not None and computed is not None:
            matches = abs(explicit - computed) <= Decimal("0.005")
            if matches:
                amount = explicit
                amount_source = "explicit"
            elif policy.trust_explicit:
                amount = explicit
                amount_source = "trusted-explicit"
            else:
                raise TtyinvError(
                    f"{section.title}, row {row_index}: explicit amount {explicit} does not match "
                    f"quantity x rate ({computed}). Use --trust-explicit to keep the written amount "
                    "or --recalculate to replace it."
                )
        elif explicit is not None:
            amount = explicit
            amount_source = "explicit"
        else:
            raise TtyinvError(
                f"{section.title}, row {row_index}: Amount is blank, but no calculable quantity and rate were found."
            )

        total += amount
        cells: list[CalculatedCell] = []
        for cell_index, cell in enumerate(row):
            if cell_index == columns.payable_amount:
                rendered = display_money(amount, invoice_currency, locale)
                cells.append(CalculatedCell(html=rendered, plain=rendered, numeric=True))
                continue
            if cell_index == columns.quantity and quantity is not None:
                rendered = display_number(quantity, locale)
                cells.append(CalculatedCell(html=rendered, plain=rendered, numeric=True))
                continue
            if cell_index == columns.rate and rate is not None:
                rendered = display_money(rate, invoice_currency, locale)
                cells.append(CalculatedCell(html=rendered, plain=rendered, numeric=True))
                continue
            column_currency = columns.currencies.get(cell_index)
            if column_currency:
                numeric = _parse_decimal(
                    cell.source,
                    f"{section.title}, row {row_index}, {section.table.headers[cell_index].source}",
                )
                if numeric is not None:
                    rendered = display_money(numeric, column_currency, locale)
                    cells.append(CalculatedCell(html=rendered, plain=rendered, numeric=True))
                    continue
            cells.append(CalculatedCell(html=cell.html, plain=cell.source, numeric=False))

        rows.append(CalculatedRow(cells=cells, amount=amount, amount_source=amount_source))

    return CalculatedFinancialSection(
        title=section.title,
        headers=[header.html for header in section.table.headers],
        align=section.table.align,
        rows=rows,
        total=total,
        payable_amount_column=columns.payable_amount,
    )


def calculate_invoice(invoice: ParsedInvoice, policy: AmountPolicy) -> CalculatedInvoice:
    if policy.trust_explicit and policy.recalculate:
        raise TtyinvError("--trust-explicit and --recalculate cannot be used together.")

    currency = invoice.frontmatter.invoice.currency
    locale = invoice.frontmatter.invoice.locale
    sections: list[CalculatedFinancialSection | CalculatedProseSection] = []
    grand_total = Decimal("0")

    for section in invoice.sections:
        if section.kind == "prose":
            sections.append(CalculatedProseSection(title=section.title, html=section.html))
            continue
        calculated = _calculate_section(section, currency, locale, policy)
        sections.append(calculated)
        grand_total += calculated.total

    return CalculatedInvoice(
        source_path=invoice.source_path,
        source_directory=invoice.source_directory,
        frontmatter=invoice.frontmatter,
        preamble_html=invoice.preamble_html,
        sections=sections,
        grand_total=grand_total,
    )
