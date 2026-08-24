"""Hardened CLI; legacy rendering remains the visual source of truth."""

from __future__ import annotations

import argparse
import contextlib
import inspect
import io
import sys
import tempfile
from enum import IntEnum
from pathlib import Path
from typing import Sequence

from . import __version__
from .appearance import resolve_palette
from .diagnostics import Diagnostic
from .html_enhance import EnhanceOptions, enhance_html
from .linting import diagnostics_json, lint_source
from .pdf_hardening import render_pdf
from .schema_v1 import schema_json
from .templates import STARTER_LOGO_SVG, STARTER_SIGNATURE_SVG, starter_invoice

_COMMANDS = {"render", "lint", "init", "schema", "fonts"}


class ExitCode(IntEnum):
    """Stable process exit codes documented as part of the CLI contract."""

    OK = 0
    USAGE = 2
    PARSE_SCHEMA = 3
    ARITHMETIC = 4
    ASSET_SECURITY = 5
    RENDER = 6
    UNEXPECTED = 70


def _appearance(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--theme", choices=("light", "dark"), default="light")
    parser.add_argument("--font", help="installed monospace font family; Geist Mono remains the default")
    parser.add_argument("--accent", help="accent CSS color")
    parser.add_argument("--paper", help="paper/background CSS color")
    parser.add_argument("--ink", help="primary text CSS color")
    parser.add_argument("--muted", help="secondary text/rule CSS color")
    parser.add_argument("--density", choices=("comfortable", "compact"), default="comfortable")


def _policy(parser: argparse.ArgumentParser) -> None:
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--trust-explicit", action="store_true")
    group.add_argument("--recalculate", action="store_true")


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="ttyinv", description="Render strict Markdown invoices to HTML and A4 PDF.")
    parser.add_argument("--version", action="version", version=f"ttyinv {__version__}")
    parser.add_argument("--list-fonts", action="store_true")
    subs = parser.add_subparsers(dest="command")

    render = subs.add_parser("render")
    render.add_argument("input", type=Path)
    render.add_argument("--format", choices=("pdf", "html", "both"), default="pdf")
    render.add_argument("--output", type=Path)
    _appearance(render)
    _policy(render)
    render.add_argument("--allow-outside-root", action="store_true")
    render.add_argument("--deterministic", action="store_true")
    render.add_argument("--browser")

    lint = subs.add_parser("lint")
    lint.add_argument("input", type=Path)
    _appearance(lint)
    _policy(lint)
    lint.add_argument("--allow-outside-root", action="store_true")
    lint.add_argument("--require-link-targets", action="store_true")
    lint.add_argument("--strict", action="store_true")
    lint.add_argument("--json", action="store_true", dest="json_output")

    init = subs.add_parser("init")
    init.add_argument("path", nargs="?", type=Path, default=Path("invoice.md"))
    init.add_argument("--force", action="store_true")
    init.add_argument("--with-assets", action="store_true", help="write fabricated logo and signature SVGs under assets/")

    schema = subs.add_parser("schema")
    schema.add_argument("--output", type=Path)
    subs.add_parser("fonts")
    return parser


def _normalize(argv: Sequence[str]) -> list[str]:
    values = list(argv)
    if values and values[0] not in _COMMANDS and not values[0].startswith("-"):
        return ["render", *values]
    return values


def _amount_policy(args: argparse.Namespace) -> str:
    return "trust-explicit" if args.trust_explicit else "recalculate" if args.recalculate else "default"


def _legacy_args(args: argparse.Namespace, stem: Path) -> list[str]:
    values = [str(args.input), "--format", "html", "--output", str(stem), "--theme", args.theme]
    if args.font:
        values += ["--font", args.font]
    if args.accent:
        values += ["--accent", args.accent]
    if args.trust_explicit:
        values.append("--trust-explicit")
    if args.recalculate:
        values.append("--recalculate")
    return values


def _legacy(argv: Sequence[str]) -> tuple[int, str, str]:
    from . import cli as legacy_cli

    stdout, stderr, old_argv = io.StringIO(), io.StringIO(), sys.argv
    sys.argv = ["ttyinv", *argv]
    code = 0
    try:
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            try:
                result = legacy_cli.main(list(argv)) if inspect.signature(legacy_cli.main).parameters else legacy_cli.main()
                code = result if isinstance(result, int) else 0
            except SystemExit as exc:
                code = int(exc.code or 0)
    finally:
        sys.argv = old_argv
    return code, stdout.getvalue(), stderr.getvalue()


def _find_html(directory: Path, stem: Path) -> Path:
    exact = stem.with_suffix(".html")
    if exact.exists():
        return exact
    candidates = sorted(directory.rglob("*.html"), key=lambda path: path.stat().st_mtime_ns, reverse=True)
    if not candidates:
        raise RuntimeError("renderer completed without producing HTML")
    return candidates[0]


def _outputs(source: Path, output: Path | None, format_name: str) -> tuple[Path | None, Path | None]:
    base = output or source.with_suffix("")
    if format_name == "html":
        return (base if base.suffix.lower() == ".html" else base.with_suffix(".html")), None
    if format_name == "pdf":
        return None, (base if base.suffix.lower() == ".pdf" else base.with_suffix(".pdf"))
    if base.suffix.lower() in {".html", ".pdf"}:
        base = base.with_suffix("")
    return base.with_suffix(".html"), base.with_suffix(".pdf")


def _show(diagnostics: list[Diagnostic]) -> None:
    for diagnostic in diagnostics:
        print(diagnostic.format(), file=sys.stderr)


def _diagnostic_exit_code(diagnostics: list[Diagnostic], *, strict: bool = False) -> int:
    failing = [item for item in diagnostics if item.severity == "error" or (strict and item.severity == "warning")]
    if not failing:
        return ExitCode.OK
    categories: set[ExitCode] = set()
    for diagnostic in failing:
        if diagnostic.code.startswith(("YAML", "SCHEMA", "MD", "COLOR", "PRINT")):
            categories.add(ExitCode.PARSE_SCHEMA)
        elif diagnostic.code.startswith("MONEY"):
            categories.add(ExitCode.ARITHMETIC)
        elif diagnostic.code.startswith(("PATH", "A11Y")):
            categories.add(ExitCode.ASSET_SECURITY)
        elif diagnostic.code.startswith("RENDER"):
            categories.add(ExitCode.RENDER)
        else:
            categories.add(ExitCode.UNEXPECTED)
    return min(categories, key=int)


def _render(args: argparse.Namespace) -> int:
    try:
        source = args.input.read_text(encoding="utf-8")
    except OSError as exc:
        print(f"ttyinv: {exc}", file=sys.stderr)
        return ExitCode.UNEXPECTED
    diagnostics = lint_source(
        args.input, source=source, allow_outside_root=args.allow_outside_root,
        amount_policy=_amount_policy(args), theme=args.theme, paper=args.paper,
        ink=args.ink, muted=args.muted, accent=args.accent,
    )
    if any(item.severity == "error" for item in diagnostics):
        _show(diagnostics)
        return _diagnostic_exit_code(diagnostics)
    _show([item for item in diagnostics if item.severity == "warning"])
    try:
        palette = resolve_palette(args.theme, paper=args.paper, ink=args.ink, muted=args.muted, accent=args.accent)
    except ValueError as exc:
        print(f"ttyinv: {exc}", file=sys.stderr)
        return ExitCode.PARSE_SCHEMA
    html_output, pdf_output = _outputs(args.input, args.output, args.format)
    link_target = html_output or pdf_output or args.input.with_suffix(".html")
    with tempfile.TemporaryDirectory(prefix="ttyinv-render-") as directory_name:
        directory, stem = Path(directory_name), Path(directory_name) / "invoice"
        code, stdout, stderr = _legacy(_legacy_args(args, stem))
        if code:
            print((stderr or stdout or "renderer failed").rstrip(), file=sys.stderr)
            return ExitCode.RENDER
        rendered = _find_html(directory, stem).read_text(encoding="utf-8")
        enhanced = enhance_html(
            rendered,
            EnhanceOptions(args.input, link_target, palette, args.density, args.allow_outside_root, args.deterministic),
        )
        if html_output:
            html_output.parent.mkdir(parents=True, exist_ok=True)
            html_output.write_text(enhanced, encoding="utf-8", newline="\n")
            print(html_output)
        if pdf_output:
            try:
                render_pdf(enhanced, pdf_output, deterministic=args.deterministic, browser_executable=args.browser)
            except Exception as exc:
                print(f"ttyinv: PDF generation failed: {exc}", file=sys.stderr)
                return ExitCode.RENDER
            print(pdf_output)
    return ExitCode.OK


def _lint(args: argparse.Namespace) -> int:
    try:
        source = args.input.read_text(encoding="utf-8")
    except OSError as exc:
        diagnostics = [Diagnostic("error", "IO001", str(exc), str(args.input))]
    else:
        diagnostics = lint_source(
            args.input, source=source, allow_outside_root=args.allow_outside_root,
            require_link_targets=args.require_link_targets, amount_policy=_amount_policy(args),
            theme=args.theme, paper=args.paper, ink=args.ink, muted=args.muted, accent=args.accent,
        )
        if not any(item.severity == "error" for item in diagnostics):
            with tempfile.TemporaryDirectory(prefix="ttyinv-lint-") as directory_name:
                code, stdout, stderr = _legacy(_legacy_args(args, Path(directory_name) / "invoice"))
                if code:
                    diagnostics.append(Diagnostic("error", "RENDER001", (stderr or stdout or "renderer validation failed").strip(), str(args.input)))
    if args.json_output:
        print(diagnostics_json(diagnostics))
    elif diagnostics:
        _show(diagnostics)
    else:
        print(f"{args.input}: ok")
    return _diagnostic_exit_code(diagnostics, strict=args.strict)


def _fonts() -> int:
    code, stdout, stderr = _legacy(["--list-fonts"])
    if stdout:
        print(stdout, end="")
    if stderr:
        print(stderr, end="", file=sys.stderr)
    return ExitCode.OK if code == 0 else ExitCode.RENDER


def _init(args: argparse.Namespace) -> int:
    assets = {
        args.path.parent / "assets" / "logo.svg": STARTER_LOGO_SVG,
        args.path.parent / "assets" / "signature.svg": STARTER_SIGNATURE_SVG,
    } if args.with_assets else {}
    targets = [args.path, *assets]
    existing = next((path for path in targets if path.exists()), None)
    if existing is not None and not args.force:
        print(f"ttyinv: {existing} already exists; pass --force to replace it", file=sys.stderr)
        return ExitCode.USAGE
    try:
        args.path.parent.mkdir(parents=True, exist_ok=True)
        if assets:
            next(iter(assets)).parent.mkdir(parents=True, exist_ok=True)
        args.path.write_text(starter_invoice(with_assets=args.with_assets), encoding="utf-8", newline="\n")
        for path, content in assets.items():
            path.write_text(content, encoding="utf-8", newline="\n")
    except OSError as exc:
        print(f"ttyinv: could not initialize starter invoice: {exc}", file=sys.stderr)
        return ExitCode.UNEXPECTED
    print(args.path)
    return ExitCode.OK


def _schema(args: argparse.Namespace) -> int:
    content = schema_json()
    if not args.output:
        print(content, end="")
        return ExitCode.OK
    try:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(content, encoding="utf-8", newline="\n")
    except OSError as exc:
        print(f"ttyinv: could not write schema: {exc}", file=sys.stderr)
        return ExitCode.UNEXPECTED
    print(args.output)
    return ExitCode.OK


def _main(argv: Sequence[str] | None = None) -> int:
    args = _parser().parse_args(_normalize(sys.argv[1:] if argv is None else argv))
    if args.list_fonts or args.command == "fonts":
        return _fonts()
    if args.command == "render":
        return _render(args)
    if args.command == "lint":
        return _lint(args)
    if args.command == "init":
        return _init(args)
    if args.command == "schema":
        return _schema(args)
    _parser().print_help()
    return ExitCode.OK


def main(argv: Sequence[str] | None = None) -> int:
    try:
        return _main(argv)
    except Exception as exc:
        print(f"ttyinv: unexpected failure: {exc}", file=sys.stderr)
        return ExitCode.UNEXPECTED


if __name__ == "__main__":
    raise SystemExit(main())
