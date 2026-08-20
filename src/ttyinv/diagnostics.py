"""Structured diagnostics for ttyinv's CLI and lint command."""

from __future__ import annotations

from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Iterable, Literal

Severity = Literal["error", "warning", "info"]


@dataclass(frozen=True, slots=True)
class Diagnostic:
    """A source-aware diagnostic suitable for humans or machine output."""

    severity: Severity
    code: str
    message: str
    path: str | None = None
    line: int | None = None
    column: int | None = None
    hint: str | None = None

    def as_dict(self) -> dict[str, object | None]:
        return asdict(self)

    def format(self) -> str:
        location = self.path or "<invoice>"
        if self.line is not None:
            location += f":{self.line}"
            if self.column is not None:
                location += f":{self.column}"
        rendered = f"{location}: {self.severity}[{self.code}]: {self.message}"
        if self.hint:
            rendered += f"\n  hint: {self.hint}"
        return rendered


class DiagnosticError(Exception):
    """Raised when one or more diagnostics prevent an operation."""

    def __init__(self, diagnostics: Iterable[Diagnostic]):
        self.diagnostics = tuple(diagnostics)
        message = "\n".join(diagnostic.format() for diagnostic in self.diagnostics)
        super().__init__(message or "ttyinv failed")


def source_path(path: Path | str | None) -> str | None:
    """Return a stable display path without resolving or leaking extra context."""

    if path is None:
        return None
    return str(Path(path))
