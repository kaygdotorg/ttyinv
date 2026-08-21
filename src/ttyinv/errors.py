from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True, slots=True)
class SourceLocation:
    path: Path | None = None
    line: int | None = None
    column: int | None = None

    def prefix(self) -> str:
        parts: list[str] = []
        if self.path is not None:
            parts.append(str(self.path))
        if self.line is not None:
            parts.append(str(self.line))
        if self.column is not None:
            parts.append(str(self.column))
        return ":".join(parts)


class TtyinvError(Exception):
    def __init__(
        self,
        message: str,
        *,
        path: str | Path | None = None,
        line: int | None = None,
        column: int | None = None,
        hint: str | None = None,
        code: str | None = None,
    ) -> None:
        super().__init__(message)
        self.message = message
        self.location = SourceLocation(Path(path) if path is not None else None, line, column)
        self.hint = hint
        self.code = code

    def __str__(self) -> str:
        prefix = self.location.prefix()
        rendered = f"{prefix}: {self.message}" if prefix else self.message
        if self.hint:
            rendered += f"\n  hint: {self.hint}"
        return rendered

    def at(
        self,
        *,
        path: str | Path | None = None,
        line: int | None = None,
        column: int | None = None,
    ) -> "TtyinvError":
        return TtyinvError(
            self.message,
            path=path if path is not None else self.location.path,
            line=line if line is not None else self.location.line,
            column=column if column is not None else self.location.column,
            hint=self.hint,
            code=self.code,
        )
