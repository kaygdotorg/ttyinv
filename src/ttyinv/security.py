"""Local-path validation and lightweight static scanning.

The renderer intentionally never fetches remote assets. Local assets and linked
files are resolved relative to the source Markdown file. Traversal outside that
root is denied unless the caller opts in explicitly.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable
from urllib.parse import unquote, urlparse

import yaml

from .diagnostics import Diagnostic
from .errors import TtyinvError

_FRONTMATTER_RE = re.compile(r"\A---[ \t]*\r?\n(?P<yaml>.*?)\r?\n---[ \t]*(?:\r?\n|\Z)", re.DOTALL)
_MARKDOWN_LINK_RE = re.compile(r"(?P<image>!)?\[(?P<label>[^\]]*)\]\((?P<target>[^)\s]+)(?:\s+[\"'][^\"']*[\"'])?\)")
_PATH_KEYS = {"logo", "image", "signature", "attachment", "asset", "icon"}
_REMOTE_SCHEMES = {"http", "https", "mailto", "tel", "data"}


@dataclass(frozen=True, slots=True)
class LocalReference:
    raw: str
    path: Path
    line: int
    column: int
    is_image: bool
    label: str


def _inside(candidate: Path, root: Path) -> bool:
    try:
        candidate.relative_to(root)
        return True
    except ValueError:
        return False


def resolve_local_reference(raw: str, *, root: Path, allow_outside_root: bool) -> tuple[Path | None, str | None]:
    target = raw.strip().strip("<>")
    if not target or target.startswith("#"):
        return None, None
    parsed = urlparse(target)
    if parsed.scheme.lower() in _REMOTE_SCHEMES:
        return None, None
    if parsed.scheme and parsed.scheme.lower() != "file":
        return None, f"unsupported link scheme {parsed.scheme!r}"
    if parsed.scheme.lower() == "file":
        candidate = Path(unquote(parsed.path)).expanduser()
    else:
        without_fragment = target.split("#", 1)[0].split("?", 1)[0]
        candidate = Path(unquote(without_fragment)).expanduser()
        if not candidate.is_absolute():
            candidate = root / candidate
    try:
        resolved = candidate.resolve(strict=False)
        resolved_root = root.resolve(strict=True)
    except OSError as exc:
        return None, f"could not resolve path: {exc}"
    if not allow_outside_root and not _inside(resolved, resolved_root):
        return resolved, "path escapes the invoice directory"
    return resolved, None


def resolve_local_path(
    reference: str,
    source_directory: Path,
    *,
    allow_outside_root: bool = False,
    purpose: str = "local file",
) -> Path:
    """Renderer-facing strict path resolver using the same sandbox as linting."""
    resolved, error = resolve_local_reference(
        reference,
        root=source_directory.resolve(),
        allow_outside_root=allow_outside_root,
    )
    if error:
        raise TtyinvError(
            f"{purpose.capitalize()} {reference!r}: {error}.",
            hint="Move the file under the invoice directory or pass --allow-outside-root explicitly.",
            code="path-outside-root",
        )
    if resolved is None:
        raise TtyinvError(f"{purpose.capitalize()} {reference!r} is not a local path.")
    return resolved


def _walk_yaml_paths(value: Any) -> Iterable[str]:
    if isinstance(value, dict):
        for child_key, child_value in value.items():
            normalized = str(child_key).strip().casefold().replace("-", "_")
            if normalized in _PATH_KEYS and isinstance(child_value, str):
                yield child_value
            yield from _walk_yaml_paths(child_value)
    elif isinstance(value, list):
        for child in value:
            yield from _walk_yaml_paths(child)


def _line_column(source: str, offset: int) -> tuple[int, int]:
    line = source.count("\n", 0, offset) + 1
    last_newline = source.rfind("\n", 0, offset)
    column = offset + 1 if last_newline < 0 else offset - last_newline
    return line, column


def scan_local_references(source_path: Path, source: str) -> list[LocalReference]:
    references: list[LocalReference] = []
    frontmatter_match = _FRONTMATTER_RE.search(source)
    if frontmatter_match:
        try:
            parsed = yaml.safe_load(frontmatter_match.group("yaml")) or {}
        except yaml.YAMLError:
            parsed = {}
        for raw in _walk_yaml_paths(parsed):
            offset = source.find(raw, frontmatter_match.start("yaml"))
            line, column = _line_column(source, max(offset, 0))
            references.append(LocalReference(raw, Path(raw), line, column, True, "asset"))
    body_start = frontmatter_match.end() if frontmatter_match else 0
    for match in _MARKDOWN_LINK_RE.finditer(source, body_start):
        raw = match.group("target")
        line, column = _line_column(source, match.start("target"))
        references.append(LocalReference(raw, Path(raw), line, column, bool(match.group("image")), match.group("label")))
    return references


def validate_local_references(
    source_path: Path,
    source: str,
    *,
    allow_outside_root: bool = False,
    require_link_targets: bool = False,
) -> list[Diagnostic]:
    root = source_path.parent.resolve()
    diagnostics: list[Diagnostic] = []
    for reference in scan_local_references(source_path, source):
        resolved, error = resolve_local_reference(reference.raw, root=root, allow_outside_root=allow_outside_root)
        if error:
            diagnostics.append(Diagnostic("error", "PATH001", f"{reference.raw!r}: {error}", str(source_path), reference.line, reference.column, "Move the file below the invoice directory or pass --allow-outside-root deliberately."))
            continue
        if resolved is None:
            continue
        if not resolved.exists() and (reference.is_image or require_link_targets):
            diagnostics.append(Diagnostic("error" if reference.is_image else "warning", "PATH002", f"local {'asset' if reference.is_image else 'link target'} does not exist: {reference.raw}", str(source_path), reference.line, reference.column))
        if reference.is_image and not reference.label.strip():
            diagnostics.append(Diagnostic("warning", "A11Y001", "Markdown image has empty alternative text", str(source_path), reference.line, reference.column, "Describe meaningful images, or keep alt text empty only when the image is decorative."))
    return diagnostics
