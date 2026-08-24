from __future__ import annotations

import html
import re

from bs4 import BeautifulSoup

from .models import CalculatedFinancialSection, CalculatedSection, Party
from .money import display_money


def esc(value: object) -> str:
    return html.escape(str(value), quote=True)


def render_party(party: Party, label: str, label_id: str) -> str:
    address = "".join(f"<div>{esc(line)}</div>" for line in party.address)
    identifiers = "".join(f"<div>{esc(key)} {esc(value)}</div>" for key, value in party.identifiers.items())
    contacts: list[str] = []
    if party.email:
        contacts.append(f'<a href="mailto:{esc(party.email)}">{esc(party.email)}</a>')
    if party.website:
        contacts.append(f'<a href="{esc(party.website)}">{esc(party.website)}</a>')
    contact_html = " · ".join(contacts)
    return f'''<section class="party" aria-labelledby="{esc(label_id)}">
  <div class="block-label" id="{esc(label_id)}">[ {esc(label)} ]</div>
  <div class="party-name">{esc(party.name)}</div>
  <div class="party-lines">{address}</div>
  {f'<div class="identifier-lines">{identifiers}</div>' if identifiers else ''}
  {f'<div class="party-contact">{contact_html}</div>' if contact_html else ''}
</section>'''


def align_class(alignment: str | None, numeric: bool) -> str:
    if numeric:
        return "numeric"
    if alignment == "right":
        return "align-right"
    if alignment == "center":
        return "align-center"
    return ""


def table_cell_html(value: str) -> str:
    parts = re.split(r"<br\s*/?>", value, flags=re.IGNORECASE)
    if len(parts) == 1:
        return value
    primary, *details = parts
    return primary + "".join(f'<span class="cell-detail">{detail}</span>' for detail in details)


def normalise_header(value: str) -> str:
    plain = BeautifulSoup(value, "html.parser").get_text(" ", strip=True)
    return re.sub(r"[^a-z0-9]", "", plain.casefold())


def is_numeric_header(value: str) -> bool:
    normalized = normalise_header(value)
    return normalized in {"amount", "days", "hours", "price", "qty", "quantity", "rate", "unitprice", "units"} or normalized.startswith("amount")


def section_id(title: str, index: int) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", title.casefold()).strip("-") or "section"
    return f"section-{index + 1}-{slug}"


def column_widths(section: CalculatedFinancialSection) -> list[float]:
    count = len(section.headers)
    if count == 2:
        return [72.0, 28.0]
    if count == 3:
        return [55.0, 22.0, 23.0]
    if count == 4:
        return [55.0, 10.0, 17.0, 18.0]
    if count == 5:
        return [45.0, 10.0, 17.0, 10.0, 18.0]
    widths = [0.0] * count
    amount_index = section.payable_amount_column
    widths[amount_index] = 18.0
    description_index = next((i for i, h in enumerate(section.headers) if normalise_header(h) in {"description", "item", "service"}), 0)
    if description_index != amount_index:
        widths[description_index] = 40.0
    role_widths = {"qty": 10.0, "quantity": 10.0, "days": 10.0, "hours": 10.0, "units": 10.0, "rate": 16.0, "unitprice": 16.0, "price": 16.0, "vat": 10.0, "tax": 10.0, "code": 10.0, "sac": 10.0, "sachsc": 10.0}
    for index, header in enumerate(section.headers):
        if widths[index] == 0:
            widths[index] = role_widths.get(normalise_header(header), 0.0)
    remaining_indexes = [i for i, width in enumerate(widths) if width == 0]
    remaining = max(8.0 * len(remaining_indexes), 100.0 - sum(widths))
    fallback = remaining / len(remaining_indexes) if remaining_indexes else 0
    for index in remaining_indexes:
        widths[index] = fallback
    total = sum(widths)
    return [round(width * 100.0 / total, 4) for width in widths]


def _colgroup(widths: list[float]) -> str:
    return "<colgroup>" + "".join(f'<col style="width:{width:g}%">' for width in widths) + "</colgroup>"


def _grid_template(widths: list[float]) -> str:
    return " ".join(f"{width:g}fr" for width in widths)


def render_summary(*, widths: list[float], amount_column: int, label: str, amount: str, css_class: str) -> str:
    count = len(widths)
    label_column = max(0, amount_column - 1)
    rule_start = label_column + 1
    rule_end = min(count + 1, amount_column + 2)
    return f'''<div class="aligned-summary {css_class}" style="grid-template-columns:{_grid_template(widths)}">
  <span class="summary-rule" style="grid-column:{rule_start} / {rule_end}"></span>
  <span class="summary-label" style="grid-column:{label_column + 1} / {amount_column + 1}">{esc(label)}</span>
  <span class="summary-amount" style="grid-column:{amount_column + 1} / {min(count + 1, amount_column + 2)}">{esc(amount)}</span>
</div>'''


def render_table(*, caption: str, headers: list[str], widths: list[float], header_classes: list[str], rows: list[list[tuple[str, str]]]) -> str:
    headings = "".join(f'<th scope="col" class="{esc(header_classes[index])}">{headers[index]}</th>' for index in range(len(headers)))
    rendered_rows: list[str] = []
    for row in rows:
        cells = "".join(f'<td class="{esc(css_class)}">{cell_html}</td>' for cell_html, css_class in row)
        rendered_rows.append(f"<tr>{cells}</tr>")
    return f'''<table class="invoice-table">
  <caption class="visually-hidden">{esc(caption)}</caption>
  {_colgroup(widths)}
  <thead><tr>{headings}</tr></thead>
  <tbody>{''.join(rendered_rows)}</tbody>
</table>'''


def render_section(*, title: str, section_id_value: str, body: str, css_class: str, attributes: str = "") -> str:
    attrs = f" {attributes.strip()}" if attributes.strip() else ""
    return f'''<section class="{esc(css_class)}" aria-labelledby="{esc(section_id_value)}"{attrs}>
  <h2 class="section-heading" id="{esc(section_id_value)}">[ {esc(title)} ]</h2>
  {body}
</section>'''


def render_financial_section(section: CalculatedFinancialSection, currency: str, locale: str, *, show_section_total: bool, grand_total: str | None, section_id_value: str) -> tuple[str, list[float]]:
    widths = column_widths(section)
    header_classes = [align_class(section.align[i] if i < len(section.align) else None, i == section.payable_amount_column or is_numeric_header(section.headers[i])) for i in range(len(section.headers))]
    rows: list[list[tuple[str, str]]] = []
    for row in section.rows:
        rendered_row: list[tuple[str, str]] = []
        for i, cell in enumerate(row.cells):
            rendered_row.append((table_cell_html(cell.html), align_class(section.align[i] if i < len(section.align) else None, cell.numeric or i == section.payable_amount_column)))
        rows.append(rendered_row)
    table = render_table(caption=section.title, headers=section.headers, widths=widths, header_classes=header_classes, rows=rows)
    authored_summary = any(row.summary_label in {"subtotal", "total", "grand total"} for row in section.rows)
    subtotal = render_summary(widths=widths, amount_column=section.payable_amount_column, label=section.title, amount=display_money(section.total, currency, locale), css_class="section-total") if show_section_total and not section.summary_only and not authored_summary else ""
    total_due = render_summary(widths=widths, amount_column=section.payable_amount_column, label="Total due", amount=grand_total, css_class="grand-total") if grand_total is not None else ""
    body = f'''{table}
<div class="section-tail">
  <div class="table-end-rule" aria-hidden="true"></div>
  {subtotal}
  {total_due}
</div>'''
    page_break = ' data-page-break-before="true"' if section.page_break_before else ''
    return render_section(title=section.title, section_id_value=section_id_value, body=body, css_class="document-section financial-section", attributes=f'data-column-widths="{",".join(f"{width:g}" for width in widths)}"{page_break}'), widths


def render_prose_section(section: CalculatedSection, section_id_value: str) -> str:
    attributes = 'data-page-break-before="true"' if section.page_break_before else ""
    return render_section(title=section.title, section_id_value=section_id_value, body=section.html, css_class="document-section prose-section", attributes=attributes)
