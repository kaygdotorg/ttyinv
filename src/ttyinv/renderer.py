from __future__ import annotations

import html
import os
import re
from pathlib import Path
from urllib.parse import urlparse

from bs4 import BeautifulSoup

from .assets import embed_local_asset
from .colors import validate_css_color
from .errors import TtyinvError
from .fonts import FontAssets, resolve_font_assets
from .models import (
    CalculatedFinancialSection,
    CalculatedInvoice,
    CalculatedSection,
    Party,
    RenderOptions,
    RenderResult,
)
from .money import display_money
from .styles import BASE_CSS


def _e(value: object) -> str:
    return html.escape(str(value), quote=True)


def _render_party(party: Party, label: str) -> str:
    address = "".join(f"<div>{_e(line)}</div>" for line in party.address)
    identifiers = "".join(
        f"<div>{_e(key)} {_e(value)}</div>" for key, value in party.identifiers.items()
    )
    contacts: list[str] = []
    if party.email:
        contacts.append(f'<a href="mailto:{_e(party.email)}">{_e(party.email)}</a>')
    if party.website:
        contacts.append(f'<a href="{_e(party.website)}">{_e(party.website)}</a>')
    contact_html = " · ".join(contacts)
    return f"""
<section class="party">
  <div class="block-label">[ {_e(label)} ]</div>
  <div class="party-name">{_e(party.name)}</div>
  <div class="party-lines">{address}</div>
  {f'<div class="identifier-lines">{identifiers}</div>' if identifiers else ''}
  {f'<div class="party-contact">{contact_html}</div>' if contact_html else ''}
</section>
"""


def _align_class(alignment: str | None, numeric: bool) -> str:
    if numeric:
        return "numeric"
    if alignment == "right":
        return "align-right"
    if alignment == "center":
        return "align-center"
    return ""


def _table_cell_html(value: str) -> str:
    parts = re.split(r"<br\s*/?>", value, flags=re.IGNORECASE)
    if len(parts) == 1:
        return value
    primary, *details = parts
    return primary + "".join(f'<span class="cell-detail">{detail}</span>' for detail in details)


def _plain_header(value: str) -> str:
    return BeautifulSoup(value, "html.parser").get_text(" ", strip=True)


def _normalise_header(value: str) -> str:
    return re.sub(r"[^a-z0-9]", "", _plain_header(value).casefold())


def _column_widths(section: CalculatedFinancialSection) -> list[float]:
    """Return one deterministic grid shared by table, subtotals and grand total."""

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

    description_index = next(
        (
            index
            for index, header in enumerate(section.headers)
            if _normalise_header(header) in {"description", "item", "service"}
        ),
        0,
    )
    if description_index != amount_index:
        widths[description_index] = 40.0

    role_widths = {
        "qty": 10.0,
        "quantity": 10.0,
        "days": 10.0,
        "hours": 10.0,
        "units": 10.0,
        "rate": 16.0,
        "unitprice": 16.0,
        "price": 16.0,
        "vat": 10.0,
        "tax": 10.0,
        "code": 10.0,
        "sac": 10.0,
        "sachsc": 10.0,
    }
    for index, header in enumerate(section.headers):
        if widths[index] == 0:
            widths[index] = role_widths.get(_normalise_header(header), 0.0)

    remaining_indexes = [index for index, width in enumerate(widths) if width == 0]
    remaining = max(8.0 * len(remaining_indexes), 100.0 - sum(widths))
    fallback = remaining / len(remaining_indexes) if remaining_indexes else 0
    for index in remaining_indexes:
        widths[index] = fallback

    total = sum(widths)
    return [round(width * 100.0 / total, 4) for width in widths]


def _colgroup(widths: list[float]) -> str:
    columns = "".join(f'<col style="width:{width:g}%">' for width in widths)
    return f"<colgroup>{columns}</colgroup>"


def _grid_template(widths: list[float]) -> str:
    return " ".join(f"{width:g}fr" for width in widths)


def _render_summary(
    *,
    widths: list[float],
    amount_column: int,
    label: str,
    amount: str,
    css_class: str,
) -> str:
    count = len(widths)
    # Summary geometry is anchored to the final two table columns. The label is
    # right-aligned in the penultimate column and may visually extend left, just
    # like the reference invoice; the amount remains flush with the table's
    # final right edge. This keeps the short rule and Total due on the exact same
    # grid as the table rather than starting one column too early.
    label_column = max(0, amount_column - 1)
    rule_start = label_column + 1
    rule_end = min(count + 1, amount_column + 2)
    label_start = label_column + 1
    label_end = amount_column + 1
    amount_start = amount_column + 1
    amount_end = min(count + 1, amount_column + 2)
    return f"""
<div class="aligned-summary {css_class}" style="grid-template-columns:{_grid_template(widths)}">
  <span class="summary-rule" style="grid-column:{rule_start} / {rule_end}"></span>
  <span class="summary-label" style="grid-column:{label_start} / {label_end}">{_e(label)}</span>
  <span class="summary-amount" style="grid-column:{amount_start} / {amount_end}">{_e(amount)}</span>
</div>
"""


def _render_financial_section(
    section: CalculatedFinancialSection,
    currency: str,
    locale: str,
    *,
    show_section_total: bool,
) -> tuple[str, list[float]]:
    widths = _column_widths(section)
    headings = "".join(
        f'<th class="{_align_class(section.align[index] if index < len(section.align) else None, index == section.payable_amount_column)}">{heading}</th>'
        for index, heading in enumerate(section.headers)
    )
    rows = []
    for row in section.rows:
        cells = "".join(
            f'<td class="{_align_class(section.align[index] if index < len(section.align) else None, cell.numeric or index == section.payable_amount_column)}">{_table_cell_html(cell.html)}</td>'
            for index, cell in enumerate(row.cells)
        )
        rows.append(f"<tr>{cells}</tr>")

    subtotal = ""
    if show_section_total:
        subtotal = _render_summary(
            widths=widths,
            amount_column=section.payable_amount_column,
            label=section.title,
            amount=display_money(section.total, currency, locale),
            css_class="section-total",
        )

    rendered = f"""
<section class="financial-section" data-column-widths="{','.join(f'{width:g}' for width in widths)}">
  <h2 class="section-heading">[ {_e(section.title)} ]</h2>
  <table class="invoice-table">
    {_colgroup(widths)}
    <thead><tr>{headings}</tr></thead>
    <tbody>{''.join(rows)}</tbody>
  </table>
  <div class="table-end-rule" aria-hidden="true"></div>
  {subtotal}
</section>
"""
    return rendered, widths


def _render_prose_section(section: CalculatedSection) -> str:
    return f"""
<section class="prose-section">
  <h2 class="section-heading">[ {_e(section.title)} ]</h2>
  {section.html}
</section>
"""


def _font_source(embedded: str | None, local_names: list[str]) -> str:
    sources: list[str] = []
    if embedded:
        sources.append(f'url("{embedded}")')
    sources.extend(f'local("{name.replace(chr(34), "")}")' for name in local_names)
    return ", ".join(sources)


def _safe_font_family(family: str) -> str:
    cleaned = re.sub(r'["\\{};\r\n]', "", family).strip()
    if not cleaned:
        raise TtyinvError("Font family name is empty after sanitisation.")
    return cleaned


def _font_css(assets: FontAssets) -> str:
    family = _safe_font_family(assets.family)
    internal_family = f"ttyinv {family}"
    local_regular = [family, f"{family} Regular"]
    local_strong = [f"{family} SemiBold", f"{family} Semibold", f"{family} Bold", family]
    regular_source = _font_source(assets.regular, local_regular)
    strong_source = _font_source(assets.strong or assets.regular, local_strong)
    return "\n".join(
        [
            (
                f'@font-face {{ font-family: "{internal_family}"; src: {regular_source}; '
                'font-style: normal; font-weight: 400; font-display: block; }}'
            ),
            (
                f'@font-face {{ font-family: "{internal_family}"; src: {strong_source}; '
                'font-style: normal; font-weight: 600; font-display: block; }}'
            ),
            (
                f'@font-face {{ font-family: "{internal_family}"; src: {strong_source}; '
                'font-style: normal; font-weight: 700; font-display: block; }}'
            ),
            (
                f':root {{ --font-family: "{internal_family}", "{family}", '
                'ui-monospace, monospace; }}'
            ),
        ]
    )


def _is_external_link(href: str) -> bool:
    parsed = urlparse(href)
    return parsed.scheme in {"http", "https", "mailto", "data"} or href.startswith("#")


def _rewrite_document(
    document: str,
    invoice: CalculatedInvoice,
    options: RenderOptions,
) -> RenderResult:
    soup = BeautifulSoup(document, "html.parser")
    warnings: list[str] = []

    for image in soup.find_all("img"):
        source = image.get("src")
        if not source or source.startswith("data:"):
            continue
        image["src"] = embed_local_asset(source, invoice.source_directory)

    for anchor in soup.find_all("a"):
        href = anchor.get("href")
        if not href or _is_external_link(href):
            continue
        parsed = urlparse(href)
        if parsed.scheme:
            raise TtyinvError(f"Unsupported link protocol in {href!r}.")
        target = (invoice.source_directory / parsed.path).resolve()
        if not target.exists():
            warnings.append(f"local link target does not exist: {href}")
        suffix = ""
        if parsed.query:
            suffix += f"?{parsed.query}"
        if parsed.fragment:
            suffix += f"#{parsed.fragment}"
        if options.for_pdf:
            anchor["href"] = f"{target.as_uri()}{suffix}"
            warnings.append(f"local file link is best effort in PDF viewers: {href}")
        else:
            relative = os.path.relpath(target, options.output_path.parent).replace(os.sep, "/")
            if not relative.startswith("."):
                relative = f"./{relative}"
            anchor["href"] = f"{relative}{suffix}"

    return RenderResult(html=str(soup), warnings=sorted(set(warnings)))


def render_html(invoice: CalculatedInvoice, options: RenderOptions) -> RenderResult:
    frontmatter = invoice.frontmatter
    meta = frontmatter.invoice
    currency = meta.currency
    locale = meta.locale

    logo = (
        embed_local_asset(frontmatter.issuer.logo, invoice.source_directory)
        if frontmatter.issuer.logo
        else None
    )
    signature_image = (
        embed_local_asset(frontmatter.signature.image, invoice.source_directory)
        if frontmatter.signature and frontmatter.signature.image
        else None
    )

    font_config = frontmatter.appearance.font if frontmatter.appearance else None
    font_assets = resolve_font_assets(
        override_family=options.font_family_override,
        config=font_config,
        source_directory=invoice.source_directory,
    )

    due_html = ""
    if meta.due:
        terms = f" · {_e(meta.terms)}" if meta.terms else ""
        due_html = f"<dt>Due</dt><dd>{_e(meta.due)}{terms}</dd>"
    elif meta.terms:
        due_html = f"<dt>Terms</dt><dd>{_e(meta.terms)}</dd>"

    financial_count = sum(section.kind == "financial" for section in invoice.sections)
    last_financial_index = max(
        index for index, section in enumerate(invoice.sections) if section.kind == "financial"
    )
    section_fragments: list[str] = []
    last_widths: list[float] = [72.0, 28.0]
    last_amount_column = 1
    for index, section in enumerate(invoice.sections):
        if section.kind == "financial":
            fragment, widths = _render_financial_section(
                section,
                currency,
                locale,
                show_section_total=financial_count > 1,
            )
            section_fragments.append(fragment)
            last_widths = widths
            last_amount_column = section.payable_amount_column
            if index == last_financial_index:
                section_fragments.append(
                    _render_summary(
                        widths=last_widths,
                        amount_column=last_amount_column,
                        label="Total due",
                        amount=display_money(invoice.grand_total, currency, locale),
                        css_class="grand-total",
                    )
                )
        else:
            section_fragments.append(_render_prose_section(section))
    sections = "\n".join(section_fragments)

    settlement_html = ""
    if frontmatter.settlements:
        rows = []
        for settlement in frontmatter.settlements:
            received = (
                display_money(settlement.received.amount, settlement.received.currency, locale)
                if settlement.received
                else "-"
            )
            rows.append(
                f"<div>{_e(settlement.date)}</div>"
                f'<div class="money">{_e(display_money(settlement.paid.amount, settlement.paid.currency, locale))}</div>'
                f'<div class="money">{_e(received)}</div>'
            )
        settlement_html = f"""
<section class="settlement-block">
  <h2 class="section-heading">[ Settlement ]</h2>
  <div class="settlement-grid">
    <div class="head">Date</div><div class="head money">Paid</div><div class="head money">Received</div>
    {''.join(rows)}
  </div>
</section>
"""

    payment_html = ""
    if frontmatter.payment and frontmatter.payment.methods:
        methods = []
        for method in frontmatter.payment.methods:
            fields = "".join(
                f"<dt>{_e(key)}</dt><dd>{_e(value)}</dd>" for key, value in method.fields.items()
            )
            methods.append(
                f'<div class="payment-method"><div class="payment-method-title">{_e(method.title)}</div>'
                f'<dl class="payment-fields">{fields}</dl></div>'
            )
        payment_html = f"""
<section class="payment-frame">
  <div class="payment-label">[ {_e(frontmatter.payment.title)} ]</div>
  {''.join(methods)}
</section>
"""

    signature_html = ""
    if frontmatter.signature:
        signature = frontmatter.signature
        signature_html = f"""
<section class="signature">
  {f'<img src="{_e(signature_image)}" alt="{_e(signature.label or "Signature")}">' if signature_image else ''}
  {f'<div class="signature-name">{_e(signature.name)}</div>' if signature.name else ''}
  {f'<div class="signature-label">{_e(signature.label)}</div>' if signature.label else ''}
</section>
"""

    accent = options.accent_override
    if accent is None and frontmatter.appearance and frontmatter.appearance.accent:
        accent = frontmatter.appearance.accent
    accent_css = ""
    if accent:
        accent = validate_css_color(accent)
        accent_css = (
            f':root, html[data-theme="light"], html[data-theme="dark"] '
            f'{{ --accent: {accent}; }}'
        )
    font_css = _font_css(font_assets)

    document = f"""<!doctype html>
<html lang="en" data-theme="{_e(options.theme)}">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="generator" content="ttyinv">
  <title>{_e(meta.number)} - {_e(meta.title)}</title>
  <style>{BASE_CSS}\n{accent_css}\n{font_css}</style>
</head>
<body>
  <article class="invoice-sheet">
    <div class="page-frame" aria-hidden="true">
      <span class="frame-edge top"></span>
      <span class="frame-edge right"></span>
      <span class="frame-edge bottom"></span>
      <span class="frame-edge left"></span>
      <span class="frame-junction tl"></span>
      <span class="frame-junction tr"></span>
      <span class="frame-junction bl"></span>
      <span class="frame-junction br"></span>
    </div>
    <div class="invoice-badge">[ {_e(meta.title)} - {_e(display_money(invoice.grand_total, currency, locale))} ]</div>
    <header class="invoice-header">
      <div class="brand-row">
        {f'<img class="brand-logo" src="{_e(logo)}" alt="">' if logo else ''}
        <div class="brand-name">{_e(frontmatter.issuer.name)}</div>
      </div>
      <dl class="invoice-meta">
        <dt>Ref</dt><dd>{_e(meta.number)}</dd>
        <dt>Issued</dt><dd>{_e(meta.issued)}</dd>
        {due_html}
      </dl>
    </header>
    <section class="parties">
      {_render_party(frontmatter.issuer, "From")}
      {_render_party(frontmatter.recipient, "Bill to")}
    </section>
    {f'<section class="preamble">{invoice.preamble_html}</section>' if invoice.preamble_html else ''}
    <main>
      {sections}
      {settlement_html}
    </main>
    <div class="footer-stack">
      {payment_html}
      {signature_html}
    </div>
  </article>
</body>
</html>"""

    result = _rewrite_document(document, invoice, options)
    result.warnings = sorted(set(result.warnings + font_assets.warnings))
    return result
