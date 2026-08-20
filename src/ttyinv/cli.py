from __future__ import annotations

from pathlib import Path

import click

from . import __version__
from .colors import validate_css_color
from .errors import TtyinvError
from .fonts import list_monospace_families
from .models import AmountPolicy, RenderOptions
from .money import calculate_invoice
from .parser import parse_invoice_file
from .pdf import render_pdf
from .renderer import render_html


def _append_extension(path: Path, extension: str) -> Path:
    if path.suffix.lower() == extension:
        return path
    return Path(f"{path}{extension}")


def _output_paths(input_path: Path, output: Path | None, output_format: str) -> dict[str, Path]:
    source_stem = input_path.with_suffix("")
    requested = output or source_stem
    if output_format == "html":
        return {"html": _append_extension(requested, ".html")}
    if output_format == "pdf":
        return {"pdf": _append_extension(requested, ".pdf")}
    if requested.suffix.lower() in {".html", ".pdf"}:
        requested = requested.with_suffix("")
    return {
        "html": _append_extension(requested, ".html"),
        "pdf": _append_extension(requested, ".pdf"),
    }


def _accent_value(_ctx: click.Context, _param: click.Parameter, value: str | None) -> str | None:
    if value is None:
        return None
    try:
        return validate_css_color(value)
    except TtyinvError as exc:
        raise click.BadParameter(str(exc)) from exc


@click.command(context_settings={"help_option_names": ["-h", "--help"]})
@click.version_option(__version__, prog_name="ttyinv")
@click.argument(
    "invoice",
    required=False,
    type=click.Path(path_type=Path, dir_okay=False, exists=True),
)
@click.option(
    "--format",
    "output_format",
    type=click.Choice(["pdf", "html", "both"], case_sensitive=False),
    default="pdf",
    show_default=True,
    help="Output format.",
)
@click.option(
    "--theme",
    type=click.Choice(["light", "dark"], case_sensitive=False),
    default="light",
    show_default=True,
    help="Invoice theme.",
)
@click.option(
    "--accent",
    callback=_accent_value,
    metavar="COLOR",
    help="Override the theme accent with a CSS color.",
)
@click.option(
    "--font",
    "font_family",
    metavar="FAMILY",
    help="Use an installed monospace font family and embed it in the output.",
)
@click.option(
    "--list-fonts",
    is_flag=True,
    help="List installed font families verified as monospace, then exit.",
)
@click.option("-o", "--output", type=click.Path(path_type=Path), help="Output path or filename stem.")
@click.option(
    "--trust-explicit",
    is_flag=True,
    help="Keep explicit amounts that differ from quantity x rate.",
)
@click.option(
    "--recalculate",
    is_flag=True,
    help="Recalculate every row that has quantity and rate.",
)
@click.option("--chromium", type=click.Path(path_type=Path), help="Path to Chromium or Google Chrome.")
def main(
    invoice: Path | None,
    output_format: str,
    theme: str,
    accent: str | None,
    font_family: str | None,
    list_fonts: bool,
    output: Path | None,
    trust_explicit: bool,
    recalculate: bool,
    chromium: Path | None,
) -> None:
    """Render INVOICE, a self-contained ttyinv Markdown file."""
    try:
        if list_fonts:
            families = list_monospace_families()
            if not families:
                raise TtyinvError("No installed Latin monospace fonts were found.")
            click.echo("\n".join(families))
            return
        if invoice is None:
            raise click.UsageError("Missing argument 'INVOICE'.")

        resolved_invoice = invoice.resolve()
        paths = _output_paths(resolved_invoice, output.resolve() if output else None, output_format)
        parsed = parse_invoice_file(resolved_invoice)
        calculated = calculate_invoice(
            parsed,
            AmountPolicy(trust_explicit=trust_explicit, recalculate=recalculate),
        )
        warnings: set[str] = set()

        def options_for(path: Path, *, for_pdf: bool) -> RenderOptions:
            return RenderOptions(
                theme=theme,
                output_path=path,
                for_pdf=for_pdf,
                accent_override=accent,
                font_family_override=font_family,
            )

        if html_path := paths.get("html"):
            html_path.parent.mkdir(parents=True, exist_ok=True)
            result = render_html(calculated, options_for(html_path, for_pdf=False))
            html_path.write_text(result.html, encoding="utf-8")
            warnings.update(result.warnings)
            click.echo(f"wrote {html_path}")

        if pdf_path := paths.get("pdf"):
            pdf_path.parent.mkdir(parents=True, exist_ok=True)
            result = render_html(calculated, options_for(pdf_path, for_pdf=True))
            warnings.update(result.warnings)
            render_pdf(result.html, pdf_path, str(chromium) if chromium else None)
            click.echo(f"wrote {pdf_path}")

        for warning in sorted(warnings):
            click.echo(f"warning: {warning}", err=True)
    except TtyinvError as exc:
        raise click.ClickException(str(exc)) from exc


if __name__ == "__main__":
    main()
