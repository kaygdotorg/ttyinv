"""Font metric inspection used by release checks and regression tests."""

from __future__ import annotations

from dataclasses import asdict, dataclass
from pathlib import Path

from fontTools.ttLib import TTFont


@dataclass(frozen=True, slots=True)
class FontMetrics:
    ascii_monospace: bool
    fixed_pitch_flag: bool
    advance_width: int | None
    units_per_em: int
    ascent: int
    descent: int
    line_gap: int
    line_height_em: float
    missing_ascii: tuple[int, ...]

    def as_dict(self) -> dict[str, object]:
        return asdict(self)


def inspect_font(path: Path) -> FontMetrics:
    font = TTFont(str(path), lazy=True)
    try:
        cmap = font.getBestCmap() or {}
        hmtx = font["hmtx"].metrics
        advances, missing = [], []
        for codepoint in range(0x20, 0x7F):
            glyph = cmap.get(codepoint)
            if glyph is None or glyph not in hmtx:
                missing.append(codepoint)
            else:
                advances.append(hmtx[glyph][0])
        unique = set(advances)
        head, hhea, post = font["head"], font["hhea"], font["post"]
        return FontMetrics(
            ascii_monospace=len(unique) == 1 and not missing,
            fixed_pitch_flag=bool(getattr(post, "isFixedPitch", 0)),
            advance_width=next(iter(unique)) if len(unique) == 1 else None,
            units_per_em=head.unitsPerEm,
            ascent=hhea.ascent,
            descent=hhea.descent,
            line_gap=hhea.lineGap,
            line_height_em=round((hhea.ascent - hhea.descent + hhea.lineGap) / head.unitsPerEm, 6),
            missing_ascii=tuple(missing),
        )
    finally:
        font.close()
