"""Hardened CLI; legacy rendering remains the visual source of truth."""

from __future__ import annotations

import argparse
import contextlib
import inspect
import io
import sys
import tempfile
from pathlib import Path
from typing import Sequence

from . import __version__
from .appearance import resolve_palette
from .diagnostics import Diagnostic
from .html_enhance import EnhanceOptions, enhance_html
from .linting import diagnostics_json, lint_source
from .pdf_hardening import render_pdf
from .schema_v1 import schema_json
from .templates import STARTER_INVOICE

_COMMANDS = {"render", "lint", "init", "schema", "fonts"}


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


def _render(args: argparse.Namespace) -> int:
    try:
        source = args.input.read_text(encoding="utf-8")
    except OSError as exc:
        print(f"ttyinv: {exc}", file=sys.stderr)
        return 1
    diagnostics = lint_source(
        args.input, source=source, allow_outside_root=args.allow_outside_root,
        amount_policy=_amount_policy(args), theme=args.theme, paper=args.paper,
        ink=args.ink, muted=args.muted, accent=args.accent,
    )
    if any(item.severity == "error" for item in diagnostics):
        _show(diagnostics)
        return 1
    _show([item for item in diagnostics if item.severity == "warning"])
    try:
        palette = resolve_palette(args.theme, paper=args.paper, ink=args.ink, muted=args.muted, accent=args.accent)
    except ValueError as exc:
        print(f"ttyinv: {exc}", file=sys.stderr)
        return 1
    html_output, pdf_output = _outputs(args.input, args.output, args.format)
    link_target = html_output or pdf_output or args.input.with_suffix(".html")
    with tempfile.TemporaryDirectory(prefix="ttyinv-render-") as directory_name:
        directory, stem = Path(directory_name), Path(directory_name) / "invoice"
        code, stdout, stderr = _legacy(_legacy_args(args, stem))
        if code:
            print((stderr or stdout or "renderer failed").rstrip(), file=sys.stderr)
            return code
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
                return 1
            print(pdf_output)
    return 0


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
    return 1 if any(item.severity == "error" for item in diagnostics) or (args.strict and any(item.severity == "warning" for item in diagnostics)) else 0


def _fonts() -> int:
    code, stdout, stderr = _legacy(["--list-fonts"])
    if stdout:
        print(stdout, end="")
    if stderr:
        print(stderr, end="", file=sys.stderr)
    return code


def main(argv: Sequence[str] | None = None) -> int:
    args = _parser().parse_args(_normalize(sys.argv[1:] if argv is None else argv))
    if args.list_fonts or args.command == "fonts":
        return _fonts()
    if args.command == "render":
        return _render(args)
    if args.command == "lint":
        return _lint(args)
    if args.command == "init":
        if args.path.exists() and not args.force:
            print(f"ttyinv: {args.path} already exists; pass --force to replace it", file=sys.stderr)
            return 1
        args.path.parent.mkdir(parents=True, exist_ok=True)
        args.path.write_text(STARTER_INVOICE, encoding="utf-8", newline="\n")
        print(args.path)
        return 0
    if args.command == "schema":
        content = schema_json()
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(content, encoding="utf-8", newline="\n")
            print(args.output)
        else:
            print(content, end="")
        return 0
    _parser().print_help()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
