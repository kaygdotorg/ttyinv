from __future__ import annotations

import os
import re
from urllib.parse import urlparse

from bs4 import BeautifulSoup

from .assets import embed_local_asset
from .colors import contrast_ratio, validate_css_color
from .dates import display_date
from .components import esc, render_financial_section, render_party, render_prose_section, render_section, render_table, section_id
from .errors import TtyinvError
from .fonts import FontAssets, resolve_font_assets
from .models import CalculatedInvoice, RenderOptions, RenderResult
from .money import display_money, money_in_words
from .security import resolve_local_path
from .styles import BASE_CSS

_THEME_COLORS = {
    "light": {"paper": "#ffffff", "ink": "#141416", "muted": "#5d5d63", "rule": "#a3a3aa", "accent": "#126aa8"},
    "dark": {"paper": "#121214", "ink": "#f2f2f3", "muted": "#aaaaaf", "rule": "#515157", "accent": "#58a9e8"},
}


def _font_source(embedded: str | None, local_names: list[str]) -> str:
    sources: list[str] = []
    if embedded:
        sources.append(f'url("{embedded}")')
    sources.extend(f'local("{name.replace(chr(34), "")}")' for name in local_names)
    return ", ".join(sources)


def _safe_font_family(family: str) -> str:
    cleaned = family.strip()
    if not cleaned:
        raise TtyinvError("Font family name is empty.")
    if len(cleaned) > 128:
        raise TtyinvError("Font family name exceeds 128 characters.")
    if any(ord(character) < 32 or ord(character) == 127 for character in cleaned):
        raise TtyinvError("Font family name contains control characters.")
    if re.search(r'''[<>&"'\\{};]''', cleaned):
        raise TtyinvError("Font family name contains unsafe HTML or CSS characters.")
    return cleaned


def _font_css(assets: FontAssets) -> str:
    family = _safe_font_family(assets.family)
    internal = f"ttyinv {family}"
    regular_source = _font_source(assets.regular, [family, f"{family} Regular"])
    strong_source = _font_source(assets.strong or assets.regular, [f"{family} SemiBold", f"{family} Semibold", f"{family} Bold", family])
    return "\n".join([
        f'@font-face {{ font-family:"{internal}"; src:{regular_source}; font-style:normal; font-weight:400; font-display:block; }}',
        f'@font-face {{ font-family:"{internal}"; src:{strong_source}; font-style:normal; font-weight:600; font-display:block; }}',
        f'@font-face {{ font-family:"{internal}"; src:{strong_source}; font-style:normal; font-weight:700; font-display:block; }}',
        f':root {{ --font-family:"{internal}", "{family}", ui-monospace, monospace; }}',
    ])


def _is_external_link(href: str) -> bool:
    parsed = urlparse(href)
    return parsed.scheme in {"http", "https", "mailto", "tel"} or href.startswith("#")


def _rewrite_document(document: str, invoice: CalculatedInvoice, options: RenderOptions) -> RenderResult:
    soup = BeautifulSoup(document, "html.parser")
    warnings: list[str] = []
    for image in soup.find_all("img"):
        source = image.get("src")
        if not source or source.startswith("data:"):
            continue
        image["src"] = embed_local_asset(source, invoice.source_directory, allow_outside_root=options.allow_outside_root)
    for anchor in soup.find_all("a"):
        href = anchor.get("href")
        if not href or _is_external_link(href):
            continue
        parsed = urlparse(href)
        if parsed.scheme:
            raise TtyinvError(f"Unsupported link protocol in {href!r}.")
        target = resolve_local_path(href.split("#", 1)[0].split("?", 1)[0], invoice.source_directory, allow_outside_root=options.allow_outside_root, purpose="link target")
        if not target.exists():
            warnings.append(f"local link target does not exist: {href}")
        suffix = (f"?{parsed.query}" if parsed.query else "") + (f"#{parsed.fragment}" if parsed.fragment else "")
        if options.for_pdf:
            # Absolute file:// links expose the rendering machine's usernames
            # and directory layout in a shared PDF. Retain the authored relative
            # reference; viewer support is best effort but no local path leaks.
            anchor["href"] = href
            warnings.append(f"relative local link is best effort in PDF viewers: {href}")
        else:
            relative = os.path.relpath(target, options.output_path.parent).replace(os.sep, "/")
            anchor["href"] = (relative if relative.startswith(".") else f"./{relative}") + suffix
    return RenderResult(html=str(soup), warnings=sorted(set(warnings)))


def _appearance_css(invoice: CalculatedInvoice, options: RenderOptions) -> tuple[str, str, list[str]]:
    appearance = invoice.frontmatter.appearance
    configured = {
        "accent": appearance.accent if appearance else None,
        "paper": appearance.paper if appearance else None,
        "ink": appearance.ink if appearance else None,
        "muted": appearance.muted if appearance else None,
        "rule": appearance.rule if appearance else None,
    }
    overrides = {
        "accent": options.accent_override,
        "paper": options.paper_override,
        "ink": options.ink_override,
        "muted": options.muted_override,
        "rule": options.rule_override,
    }
    selected: dict[str, str] = {}
    declarations: list[str] = []
    for name in ("accent", "paper", "ink", "muted", "rule"):
        value = overrides[name] or configured[name]
        if value is not None:
            selected[name] = validate_css_color(value)
            declarations.append(f"--{name}:{selected[name]};")
    css = ':root,html[data-theme="light"],html[data-theme="dark"]{' + "".join(declarations) + "}" if declarations else ""
    density = options.density_override or (appearance.density if appearance else None) or "comfortable"
    effective = dict(_THEME_COLORS[options.theme]); effective.update(selected)
    warnings: list[str] = []
    for foreground, minimum in (("ink", 4.5), ("muted", 3.0), ("accent", 3.0)):
        ratio = contrast_ratio(effective[foreground], effective["paper"])
        if ratio is not None and ratio < minimum:
            warnings.append(f"{foreground} to paper contrast is {ratio:.2f}:1; expected at least {minimum:.1f}:1")
    return css, density, warnings


def render_html(invoice: CalculatedInvoice, options: RenderOptions) -> RenderResult:
    frontmatter = invoice.frontmatter
    meta = frontmatter.invoice
    currency, locale = meta.currency, meta.locale
    logo = embed_local_asset(frontmatter.issuer.logo, invoice.source_directory, allow_outside_root=options.allow_outside_root) if frontmatter.issuer.logo else None
    signature_image = embed_local_asset(frontmatter.signature.image, invoice.source_directory, allow_outside_root=options.allow_outside_root) if frontmatter.signature and frontmatter.signature.image else None
    font_assets = resolve_font_assets(override_family=options.font_family_override, config=frontmatter.appearance.font if frontmatter.appearance else None, source_directory=invoice.source_directory, allow_outside_root=options.allow_outside_root)

    due_html = ""
    if meta.due:
        terms = f" · {esc(meta.terms)}" if meta.terms else ""
        due_html = f"<dt>Due</dt><dd>{esc(display_date(meta.due))}{terms}</dd>"
    elif meta.terms:
        due_html = f"<dt>Terms</dt><dd>{esc(meta.terms)}</dd>"

    financial_count = sum(section.kind == "financial" for section in invoice.sections)
    last_financial_index = max(i for i, section in enumerate(invoice.sections) if section.kind == "financial")
    fragments: list[str] = []
    for index, section in enumerate(invoice.sections):
        sid = section_id(section.title, index)
        if section.kind == "financial":
            fragment, _ = render_financial_section(section, currency, locale, show_section_total=financial_count > 1, grand_total=display_money(invoice.grand_total, currency, locale) if index == last_financial_index else None, section_id_value=sid)
            fragments.append(fragment)
        else:
            fragments.append(render_prose_section(section, sid))
    sections_html = "\n".join(fragments)

    amount_words_html = render_section(title="Amount in words", section_id_value="amount-words-label", body=f"<p>{esc(money_in_words(invoice.grand_total, currency))}</p>", css_class="document-section amount-words")

    settlement_html = ""
    if frontmatter.settlements:
        rows: list[list[tuple[str, str]]] = []
        for settlement in frontmatter.settlements:
            received = display_money(settlement.received.amount, settlement.received.currency, locale) if settlement.received else "-"
            rows.append([(esc(display_date(settlement.date)), ""), (esc(display_money(settlement.paid.amount, settlement.paid.currency, locale)), "numeric"), (esc(received), "numeric")])
        table = render_table(caption="Settlement records", headers=["Date", "Paid", "Received"], widths=[34.0, 33.0, 33.0], header_classes=["", "numeric", "numeric"], rows=rows)
        settlement_html = render_section(title="Settlement", section_id_value="settlement-label", body=f'{table}<div class="table-end-rule" aria-hidden="true"></div>', css_class="document-section settlement-block")

    settlement_words_html = ""
    if meta.kind == "gst" and frontmatter.settlements:
        received_values = [item.received for item in frontmatter.settlements if item.received is not None]
        if received_values:
            latest = received_values[-1]
            settlement_words_html = render_section(title="Settlement in words", section_id_value="settlement-words-label", body=f"<p>{esc(money_in_words(latest.amount, latest.currency))}</p>", css_class="document-section amount-words settlement-words")

    payment_html = ""
    payment_break_before = False
    if frontmatter.payment and frontmatter.payment.methods:
        methods: list[str] = []
        for method_index, method in enumerate(frontmatter.payment.methods):
            fields = "".join(f"<dt>{esc(key)}</dt><dd>{esc(value)}</dd>" for key, value in method.fields.items())
            title_id = f"payment-method-{method_index + 1}"
            methods.append(f'<div class="payment-method" aria-labelledby="{title_id}"><div class="payment-method-title" id="{title_id}">{esc(method.title)}</div><dl class="payment-fields">{fields}</dl></div>')
        payment_title = frontmatter.payment.title.strip() or "Payment Methods"
        if payment_title.casefold() == "payment":
            payment_title = "Payment Methods"
        payment_break_before = frontmatter.payment.page_break_before
        payment_html = render_section(title=payment_title, section_id_value="payment-label", body="".join(methods), css_class="payment-frame")

    signature_html = ""
    if frontmatter.signature:
        signature = frontmatter.signature
        alt = signature.label or (f"Signature of {signature.name}" if signature.name else "Signature")
        signature_html = f'''<section class="signature" aria-label="{esc(alt)}">
  {f'<img src="{esc(signature_image)}" alt="{esc(alt)}">' if signature_image else ''}
  {f'<div class="signature-name">{esc(signature.name)}</div>' if signature.name else ''}
  {f'<div class="signature-label">{esc(signature.label)}</div>' if signature.label else ''}
</section>'''

    appearance_css, density, appearance_warnings = _appearance_css(invoice, options)
    font_css = _font_css(font_assets)
    document = f'''<!doctype html>
<html lang="{esc(locale.split("-")[0])}" data-theme="{esc(options.theme)}" data-density="{esc(density)}" data-kind="{esc(meta.kind)}">
<head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="generator" content="ttyinv"><title>{esc(meta.number)} - {esc(meta.title)}</title><style>{BASE_CSS}\n{appearance_css}\n{font_css}</style></head>
<body>
<article class="invoice-sheet" aria-labelledby="invoice-title">
  <div class="page-frame" aria-hidden="true"></div>
  <span class="frame-corner tl" aria-hidden="true">+</span><span class="frame-corner tr" aria-hidden="true">+</span><span class="frame-corner bl" aria-hidden="true">+</span><span class="frame-corner br" aria-hidden="true">+</span>
  <header class="invoice-header"><div class="brand-row">{f'<img class="brand-logo" src="{esc(logo)}" alt="{esc(frontmatter.issuer.name)} logo">' if logo else ''}<div class="brand-name">{esc(frontmatter.issuer.name)}</div></div><dl class="invoice-meta"><dt>Ref</dt><dd>{esc(meta.number)}</dd><dt>Issued</dt><dd>{esc(display_date(meta.issued))}</dd>{due_html}</dl></header>
  <section class="parties">{render_party(frontmatter.issuer, "From", "from-label")}{render_party(frontmatter.recipient, "Bill to", "bill-to-label")}</section>
  {f'<section class="preamble">{invoice.preamble_html}</section>' if invoice.preamble_html else ''}
  <main>{sections_html}{amount_words_html}{settlement_html}{settlement_words_html}</main>
  <div class="footer-stack"{' data-page-break-before="true"' if payment_break_before else ''}>{payment_html}{signature_html}</div>
</article>
</body></html>'''
    result = _rewrite_document(document, invoice, options)
    result.warnings = sorted(set(result.warnings + invoice.warnings + font_assets.warnings + appearance_warnings))
    return result
