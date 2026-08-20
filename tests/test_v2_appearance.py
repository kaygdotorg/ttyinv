from __future__ import annotations

import pytest

from ttyinv.appearance import contrast_ratio, resolve_palette, validate_css_color


def test_safe_css_colors_are_accepted() -> None:
    assert validate_css_color("#ff6b57") == "#ff6b57"
    assert validate_css_color("rebeccapurple") == "rebeccapurple"
    assert validate_css_color("oklch(70% 0.14 220)") == "oklch(70% 0.14 220)"


@pytest.mark.parametrize("value", ["red;display:none", "url(https://example.com/x)", "var(--secret)", "}"])
def test_css_injection_is_rejected(value: str) -> None:
    with pytest.raises(ValueError):
        validate_css_color(value)


def test_palette_overrides_do_not_change_unspecified_tokens() -> None:
    palette = resolve_palette("dark", accent="#ff6b57")
    assert palette.paper == "#161618"
    assert palette.accent == "#ff6b57"


def test_contrast_ratio_uses_wcag_luminance() -> None:
    assert contrast_ratio("#000000", "#ffffff") == pytest.approx(21.0)
