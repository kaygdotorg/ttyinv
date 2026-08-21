from __future__ import annotations

import os
import subprocess
from dataclasses import dataclass, field
from functools import lru_cache
from importlib import resources
from pathlib import Path
from typing import Iterable

from fontTools.ttLib import TTFont

from .assets import data_url, embed_asset_path
from .errors import TtyinvError
from .models import FontConfig
from .security import resolve_local_path

GEIST_MONO_FAMILY = "Geist Mono"
SUPPORTED_FONT_FAMILIES = ("Geist Mono", "Azeret Mono", "Maple Mono")
_GEIST_REGULAR = "fonts/GeistMono-Regular.woff2"
_GEIST_SEMIBOLD = "fonts/GeistMono-SemiBold.woff2"
_FONT_SUFFIXES = {".ttf", ".otf", ".woff", ".woff2"}
_MONO_SAMPLE = " ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789.,:;!?+-=*/()[]{}"


@dataclass(frozen=True, slots=True)
class FontFace:
    path: Path
    family: str
    subfamily: str
    weight: int
    italic: bool
    advance_width: int
    units_per_em: int
    ascender: int
    descender: int
    line_gap: int


@dataclass(slots=True)
class FontAssets:
    family: str
    regular: str | None = None
    strong: str | None = None
    warnings: list[str] = field(default_factory=list)


def _read_package_font(relative_path: str) -> str | None:
    try:
        resource = resources.files("ttyinv").joinpath(relative_path)
        if not resource.is_file(): return None
        return data_url(resource.read_bytes(), "font/woff2")
    except (FileNotFoundError, ModuleNotFoundError, OSError): return None


def _debug_name(font: TTFont, *name_ids: int) -> str | None:
    table = font.get("name")
    if table is None: return None
    for name_id in name_ids:
        value = table.getDebugName(name_id)
        if value and value.strip(): return value.strip()
    return None


def _is_italic(font: TTFont, subfamily: str) -> bool:
    head = font.get("head")
    if head is not None and int(getattr(head, "macStyle", 0)) & 0x02: return True
    os2 = font.get("OS/2")
    if os2 is not None and int(getattr(os2, "fsSelection", 0)) & 0x01: return True
    return "italic" in subfamily.casefold() or "oblique" in subfamily.casefold()


def _weight(font: TTFont, subfamily: str) -> int:
    os2 = font.get("OS/2"); value = int(getattr(os2, "usWeightClass", 0)) if os2 is not None else 0
    if value: return value
    lowered = subfamily.casefold()
    if "thin" in lowered: return 100
    if "extra light" in lowered or "extralight" in lowered: return 200
    if "light" in lowered: return 300
    if "semi bold" in lowered or "semibold" in lowered or "demi" in lowered: return 600
    if "bold" in lowered: return 700
    if "black" in lowered or "heavy" in lowered: return 900
    return 400


def _ascii_advances(font: TTFont) -> set[int]:
    cmap = font.getBestCmap() or {}; hmtx = font.get("hmtx")
    if hmtx is None: return set()
    advances: set[int] = set()
    for character in _MONO_SAMPLE:
        glyph_name = cmap.get(ord(character))
        if glyph_name is None: return set()
        width, _ = hmtx.metrics.get(glyph_name, (0, 0))
        if width <= 0: return set()
        advances.add(int(width))
    return advances


def inspect_font(path: Path) -> FontFace:
    try: font = TTFont(str(path), lazy=True, recalcBBoxes=False, recalcTimestamp=False)
    except Exception as exc: raise TtyinvError(f"Cannot inspect font {path}: {exc}") from exc
    try:
        family = _debug_name(font, 16, 1)
        if not family: raise TtyinvError(f"Font {path} has no readable family name.")
        subfamily = _debug_name(font, 17, 2) or "Regular"; advances = _ascii_advances(font); post = font.get("post")
        fixed_pitch = bool(post is not None and int(getattr(post, "isFixedPitch", 0)))
        os2 = font.get("OS/2"); panose_mono = bool(os2 is not None and getattr(os2, "panose", None) is not None and int(getattr(os2.panose, "bProportion", 0)) == 9)
        if not advances or (len(advances) != 1 and not (fixed_pitch and len(advances) <= 2) and not (panose_mono and len(advances) <= 2)):
            raise TtyinvError(f"Font {family!r} is not a Latin monospace font.")
        head = font.get("head"); hhea = font.get("hhea")
        return FontFace(path.resolve(), family, subfamily, _weight(font, subfamily), _is_italic(font, subfamily), max(advances), int(getattr(head, "unitsPerEm", 1000)) if head is not None else 1000, int(getattr(hhea, "ascent", 0)) if hhea is not None else 0, int(getattr(hhea, "descent", 0)) if hhea is not None else 0, int(getattr(hhea, "lineGap", 0)) if hhea is not None else 0)
    finally: font.close()


def _fc_font_paths() -> Iterable[Path]:
    try: completed = subprocess.run(["fc-list", "--format=%{file}\n"], check=False, capture_output=True, text=True, timeout=15)
    except (FileNotFoundError, OSError, subprocess.SubprocessError): return []
    if completed.returncode != 0: return []
    return [Path(line.strip()) for line in completed.stdout.splitlines() if line.strip()]


def _common_font_directories() -> list[Path]:
    home = Path.home(); directories = [home / ".fonts", home / ".local/share/fonts", Path("/usr/share/fonts"), Path("/usr/local/share/fonts"), Path("/Library/Fonts"), Path("/System/Library/Fonts"), home / "Library/Fonts"]
    windows = os.environ.get("WINDIR")
    if windows: directories.append(Path(windows) / "Fonts")
    return directories


@lru_cache(maxsize=1)
def system_font_faces() -> tuple[FontFace, ...]:
    paths: set[Path] = set()
    for path in _fc_font_paths():
        if path.suffix.lower() in _FONT_SUFFIXES and path.is_file(): paths.add(path.resolve())
    if not paths:
        for directory in _common_font_directories():
            if not directory.is_dir(): continue
            for path in directory.rglob("*"):
                if path.suffix.lower() in _FONT_SUFFIXES and path.is_file(): paths.add(path.resolve())
    faces: list[FontFace] = []
    for path in sorted(paths):
        try: faces.append(inspect_font(path))
        except TtyinvError: continue
    return tuple(faces)


def list_monospace_families() -> list[str]: return sorted({face.family for face in system_font_faces()}, key=str.casefold)

def font_metric_signature(face: FontFace) -> tuple[float, float, float, float]:
    units = max(1, face.units_per_em); return (round(face.advance_width / units, 6), round(face.ascender / units, 6), round(face.descender / units, 6), round(face.line_gap / units, 6))

def _family_faces(family: str) -> list[FontFace]:
    requested = family.casefold().strip(); exact = [face for face in system_font_faces() if face.family.casefold() == requested]
    if exact: return exact
    normalised = requested.replace(" ", ""); return [face for face in system_font_faces() if face.family.casefold().replace(" ", "") == normalised]

def _select_faces(family: str) -> tuple[FontFace, FontFace]:
    faces = [face for face in _family_faces(family) if not face.italic]
    if not faces:
        available = list_monospace_families(); suggestions = [name for name in available if family.casefold() in name.casefold()][:5]; suffix = f" Did you mean: {', '.join(suggestions)}?" if suggestions else ""
        raise TtyinvError(f"Installed font {family!r} was not found or is not a Latin monospace font.{suffix}")
    return min(faces, key=lambda f: (abs(f.weight-400), f.weight)), min(faces, key=lambda f: (abs(f.weight-600), abs(f.weight-700)))

def system_font_assets(family: str) -> FontAssets:
    regular, strong = _select_faces(family); return FontAssets(regular.family, embed_asset_path(regular.path), embed_asset_path(strong.path))

def _resolve_configured_path(reference: str, source_directory: Path, *, allow_outside_root: bool) -> Path:
    path = resolve_local_path(reference, source_directory, allow_outside_root=allow_outside_root, purpose="font")
    if not path.is_file(): raise TtyinvError(f"Configured font file does not exist: {reference}")
    return path

def configured_font_assets(config: FontConfig, source_directory: Path, *, allow_outside_root: bool = False) -> FontAssets:
    if config.regular:
        regular_path = _resolve_configured_path(config.regular, source_directory, allow_outside_root=allow_outside_root); regular_face = inspect_font(regular_path)
        strong_path = _resolve_configured_path(config.bold, source_directory, allow_outside_root=allow_outside_root) if config.bold else regular_path; strong_face = inspect_font(strong_path); family = config.family or regular_face.family
        return FontAssets(family, embed_asset_path(regular_face.path), embed_asset_path(strong_face.path))
    if config.bold: raise TtyinvError("appearance.font.bold requires appearance.font.regular.")
    if config.family: return system_font_assets(config.family)
    raise TtyinvError("appearance.font must specify family or regular.")

def geist_mono_assets() -> FontAssets:
    regular = _read_package_font(_GEIST_REGULAR); strong = _read_package_font(_GEIST_SEMIBOLD)
    if regular and strong: return FontAssets(GEIST_MONO_FAMILY, regular, strong)
    try:
        assets = system_font_assets(GEIST_MONO_FAMILY); assets.warnings.append("bundled Geist Mono assets are unavailable; using the installed Geist Mono family"); return assets
    except TtyinvError: pass
    priorities = ["Noto Mono", "DejaVu Sans Mono", "Liberation Mono"]; available = set(list_monospace_families()); fallback = next((f for f in priorities if f in available), None) or (sorted(available, key=str.casefold)[0] if available else None)
    if fallback is None: return FontAssets(GEIST_MONO_FAMILY, warnings=["bundled Geist Mono assets are unavailable and no verifiable system monospace font was found"])
    assets = system_font_assets(fallback); assets.warnings.append(f"bundled Geist Mono assets are unavailable; using verified system fallback {fallback!r}"); return assets

def validate_supported_font_family(family: str) -> str:
    requested = family.casefold().strip()
    for supported in SUPPORTED_FONT_FAMILIES:
        if supported.casefold() == requested: return supported
    raise TtyinvError(f"Font {family!r} is not in ttyinv's calibrated font set. Supported: {', '.join(SUPPORTED_FONT_FAMILIES)}.")

def resolve_font_assets(*, override_family: str | None, config: FontConfig | None, source_directory: Path, allow_outside_root: bool = False) -> FontAssets:
    if override_family: return system_font_assets(validate_supported_font_family(override_family))
    if config: return configured_font_assets(config, source_directory, allow_outside_root=allow_outside_root)
    return geist_mono_assets()
