from __future__ import annotations

import re

from .errors import TtyinvError

_HEX_RE = re.compile(r"^#[0-9a-fA-F]{3,8}$")
_NAME_RE = re.compile(r"^[A-Za-z][A-Za-z0-9-]*$")
_FUNCTION_RE = re.compile(
    r"^(?:rgb|rgba|hsl|hsla|hwb|lab|lch|oklab|oklch|color)"
    r"\([0-9A-Za-z.%+\-, /]+\)$",
    re.IGNORECASE,
)
_FORBIDDEN = set(";{}<>\\\"'\n\r\t")


def validate_css_color(value: str) -> str:
    """Return a conservative, injection-safe CSS color value.

    ttyinv deliberately accepts modern CSS color notation, but it does not accept
    arbitrary CSS. Keeping this parser narrow lets us interpolate the value into
    the generated self-contained stylesheet safely.
    """

    color = value.strip()
    if not color:
        raise TtyinvError("Accent color cannot be empty.")
    if len(color) > 96 or any(character in _FORBIDDEN for character in color):
        raise TtyinvError(f"Invalid accent color {value!r}.")
    if _HEX_RE.fullmatch(color) or _NAME_RE.fullmatch(color) or _FUNCTION_RE.fullmatch(color):
        return color
    raise TtyinvError(
        f"Invalid accent color {value!r}. Use a CSS color such as #58a9e8, "
        "rebeccapurple, rgb(...), hsl(...), or oklch(...)."
    )
