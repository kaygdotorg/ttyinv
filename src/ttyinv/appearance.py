"""Safe appearance overrides and contrast checks."""

from __future__ import annotations

import colorsys
import re
from dataclasses import dataclass

from .diagnostics import Diagnostic

# These defaults intentionally mirror the calibrated renderer.  The hardening
# layer must never silently recolor a document after the visual renderer has
# produced it.
_DEFAULTS = {
    "light": {"paper": "#ffffff", "ink": "#141416", "muted": "#5d5d63", "accent": "#126aa8"},
    "dark": {"paper": "#121214", "ink": "#f2f2f3", "muted": "#aaaaaf", "accent": "#58a9e8"},
}
_NAMED = {
    "black": (0, 0, 0), "white": (255, 255, 255), "red": (255, 0, 0),
    "green": (0, 128, 0), "blue": (0, 0, 255), "gray": (128, 128, 128),
    "grey": (128, 128, 128), "rebeccapurple": (102, 51, 153), "transparent": (0, 0, 0),
}
_SAFE_FUNCTION = re.compile(r"^(?:rgb|rgba|hsl|hsla|hwb|lab|lch|oklab|oklch|color)\([0-9a-zA-Z.%+\-/, ]+\)$")
_SAFE_NAME = re.compile(r"^[a-zA-Z]+$")
_SAFE_HEX = re.compile(r"^#[0-9a-fA-F]{3,8}$")


@dataclass(frozen=True, slots=True)
class Palette:
    paper: str
    ink: str
    muted: str
    accent: str


def validate_css_color(value: str) -> str:
    candidate = value.strip()
    if not candidate:
        raise ValueError("color cannot be empty")
    lowered = candidate.casefold()
    if any(token in lowered for token in (";", "{", "}", "url(", "var(", "expression(")):
        raise ValueError("unsafe CSS color expression")
    if _SAFE_HEX.fullmatch(candidate) or _SAFE_NAME.fullmatch(candidate) or _SAFE_FUNCTION.fullmatch(candidate):
        return candidate
    raise ValueError("expected a CSS color such as #126aa8, rebeccapurple, rgb(...), hsl(...), or oklch(...)")


def resolve_palette(
    theme: str,
    *,
    paper: str | None = None,
    ink: str | None = None,
    muted: str | None = None,
    accent: str | None = None,
) -> Palette:
    if theme not in _DEFAULTS:
        raise ValueError(f"unknown theme {theme!r}")
    base = _DEFAULTS[theme]
    return Palette(
        paper=validate_css_color(paper or base["paper"]),
        ink=validate_css_color(ink or base["ink"]),
        muted=validate_css_color(muted or base["muted"]),
        accent=validate_css_color(accent or base["accent"]),
    )


def _parse_channel(value: str) -> float:
    value = value.strip()
    if value.endswith("%"):
        return max(0.0, min(255.0, float(value[:-1]) * 2.55))
    return max(0.0, min(255.0, float(value)))


def to_rgb(value: str) -> tuple[int, int, int] | None:
    candidate = value.strip().casefold()
    if candidate in _NAMED:
        return _NAMED[candidate]
    if candidate.startswith("#"):
        raw = candidate[1:]
        if len(raw) in {3, 4}:
            raw = "".join(ch * 2 for ch in raw[:3])
        elif len(raw) in {6, 8}:
            raw = raw[:6]
        else:
            return None
        return tuple(int(raw[index:index + 2], 16) for index in (0, 2, 4))  # type: ignore[return-value]
    rgb_match = re.fullmatch(r"rgba?\(([^)]+)\)", candidate)
    if rgb_match:
        parts = re.split(r"[, /]+", rgb_match.group(1).strip())
        if len(parts) >= 3:
            try:
                return tuple(round(_parse_channel(part)) for part in parts[:3])  # type: ignore[return-value]
            except ValueError:
                return None
    hsl_match = re.fullmatch(r"hsla?\(([^)]+)\)", candidate)
    if hsl_match:
        parts = re.split(r"[, /]+", hsl_match.group(1).strip())
        if len(parts) >= 3 and parts[1].endswith("%") and parts[2].endswith("%"):
            try:
                hue = float(parts[0].removesuffix("deg")) % 360 / 360
                saturation = float(parts[1][:-1]) / 100
                lightness = float(parts[2][:-1]) / 100
                red, green, blue = colorsys.hls_to_rgb(hue, lightness, saturation)
                return round(red * 255), round(green * 255), round(blue * 255)
            except ValueError:
                return None
    return None


def _luminance(rgb: tuple[int, int, int]) -> float:
    values = []
    for channel in rgb:
        normalized = channel / 255
        values.append(normalized / 12.92 if normalized <= 0.04045 else ((normalized + 0.055) / 1.055) ** 2.4)
    return 0.2126 * values[0] + 0.7152 * values[1] + 0.0722 * values[2]


def contrast_ratio(foreground: str, background: str) -> float | None:
    fg = to_rgb(foreground)
    bg = to_rgb(background)
    if fg is None or bg is None:
        return None
    light, dark = sorted((_luminance(fg), _luminance(bg)), reverse=True)
    return (light + 0.05) / (dark + 0.05)


def contrast_diagnostics(palette: Palette, path: str | None = None) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    ink_ratio = contrast_ratio(palette.ink, palette.paper)
    if ink_ratio is not None and ink_ratio < 4.5:
        diagnostics.append(Diagnostic("warning", "A11Y002", f"ink/paper contrast is {ink_ratio:.2f}:1; 4.5:1 or greater is recommended", path))
    accent_ratio = contrast_ratio(palette.accent, palette.paper)
    if accent_ratio is not None and accent_ratio < 3:
        diagnostics.append(Diagnostic("warning", "A11Y003", f"accent/paper contrast is {accent_ratio:.2f}:1; 3:1 or greater is recommended", path))
    return diagnostics
