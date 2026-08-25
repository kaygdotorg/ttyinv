from __future__ import annotations

from datetime import date, datetime
import re


_DATE_PATTERN = re.compile(r"\A\d{4}-\d{2}-\d{2}\Z")


def canonical_date(value: object) -> str:
    """Return a structured date in canonical ISO form or raise ValueError."""
    if isinstance(value, datetime):
        raise ValueError("date must use YYYY-MM-DD")
    if isinstance(value, date):
        return value.isoformat()
    if not isinstance(value, str) or not _DATE_PATTERN.fullmatch(value):
        raise ValueError("date must use YYYY-MM-DD")
    try:
        date.fromisoformat(value)
    except ValueError as exc:
        raise ValueError("date must be a real calendar date") from exc
    return value


def display_date(value: str) -> str:
    """Display a validated structured date without applying locale formatting."""
    return canonical_date(value)
