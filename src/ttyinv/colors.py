from __future__ import annotations

import math
import re
from typing import TypeAlias

from .errors import TtyinvError

RGB: TypeAlias = tuple[float, float, float]

_HEX_RE = re.compile(r"^#[0-9a-fA-F]{3,8}$")
_NAME_RE = re.compile(r"^[A-Za-z][A-Za-z0-9-]*$")
_FUNCTION_RE = re.compile(
    r"^(?:rgb|rgba|hsl|hsla|hwb|lab|lch|oklab|oklch|color)"
    r"\([0-9A-Za-z.%+\-, /]+\)$",
    re.IGNORECASE,
)
_RGB_RE = re.compile(r"^rgba?\((?P<body>[^)]+)\)$", re.IGNORECASE)
_FORBIDDEN = set(";{}<>\\\"'\n\r\t")
_BASIC_NAMES: dict[str, RGB] = {
    "black": (0.0, 0.0, 0.0),
    "white": (1.0, 1.0, 1.0),
    "red": (1.0, 0.0, 0.0),
    "green": (0.0, 0.5019608, 0.0),
    "blue": (0.0, 0.0, 1.0),
    "gray": (0.5019608, 0.5019608, 0.5019608),
    "grey": (0.5019608, 0.5019608, 0.5019608),
    "transparent": (1.0, 1.0, 1.0),
}


def validate_css_color(value: str) -> str:
    color = value.strip()
    if not color:
        raise TtyinvError("Color cannot be empty.")
    if len(color) > 96 or any(character in _FORBIDDEN for character in color):
        raise TtyinvError(f"Invalid CSS color {value!r}.")
    if _HEX_RE.fullmatch(color) or _NAME_RE.fullmatch(color) or _FUNCTION_RE.fullmatch(color):
        return color
    raise TtyinvError(
        f"Invalid CSS color {value!r}. Use a color such as #58a9e8, "
        "rebeccapurple, rgb(...), hsl(...), or oklch(...)."
    )


def _component(value: str) -> float | None:
    token = value.strip()
    try:
        if token.endswith("%"):
            return max(0.0, min(1.0, float(token[:-1]) / 100.0))
        return max(0.0, min(1.0, float(token) / 255.0))
    except ValueError:
        return None


def parse_rgb(value: str) -> RGB | None:
    color = value.strip().casefold()
    if color in _BASIC_NAMES:
        return _BASIC_NAMES[color]
    if _HEX_RE.fullmatch(color):
        raw = color[1:]
        if len(raw) in {3, 4}:
            raw = "".join(character * 2 for character in raw)
        if len(raw) not in {6, 8}:
            return None
        return tuple(int(raw[index : index + 2], 16) / 255.0 for index in (0, 2, 4))  # type: ignore[return-value]
    match = _RGB_RE.fullmatch(color)
    if not match:
        return None
    body = match.group("body").split("/", 1)[0]
    parts = [part for part in re.split(r"[\s,]+", body.strip()) if part]
    if len(parts) != 3:
        return None
    components = tuple(_component(part) for part in parts)
    if any(component is None for component in components):
        return None
    return components  # type: ignore[return-value]


def _linear(component: float) -> float:
    return component / 12.92 if component <= 0.04045 else math.pow((component + 0.055) / 1.055, 2.4)


def relative_luminance(rgb: RGB) -> float:
    red, green, blue = (_linear(component) for component in rgb)
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue


def contrast_ratio(foreground: str, background: str) -> float | None:
    foreground_rgb = parse_rgb(foreground)
    background_rgb = parse_rgb(background)
    if foreground_rgb is None or background_rgb is None:
        return None
    lighter = max(relative_luminance(foreground_rgb), relative_luminance(background_rgb))
    darker = min(relative_luminance(foreground_rgb), relative_luminance(background_rgb))
    return (lighter + 0.05) / (darker + 0.05)
