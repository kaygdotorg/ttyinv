# Design notes

The primary theme is a terminal-like invoice composition: Geist Mono typography, left-inset bracketed labels, dashed rules, tabular numbers, and one restrained accent color.

The light theme is canonical for printing. The dark theme uses the same measurements, font weights, line wrapping, label geometry, and pagination. Section labels such as `[ Contract fees ]`, `[ Notes ]`, `[ Settlement ]`, and `[ Payment ]` share one 4 mm left inset.

The outer page frame is independent from invoice tables. It is always present and is built from four dashed edges plus four CSS stroke junctions; corner `+` intersections must not be font glyphs. Financial tables intentionally have no outer boxes. Their header rule, body-end rule, numeric alignment, and total block all derive from the same column grid. The grand-total short rule is anchored to the final two table columns so it cannot drift from the amount column.

The implementation is being calibrated against these public visual references supplied during development:

- https://x.com/kairevicius/status/2089701163117494735
- https://x.com/emilkowalski/status/2089372767934115883
- https://x.com/kairevicius/status/2089751940221587696
- https://oxide.computer/ (broader terminal/diagram visual-language inspiration)

Before a public release presents the theme as an exact reproduction, confirm the desired permission and attribution language with the original designers. No private invoice data or reference logos are included. Geist Mono is vendored only from its official release and retains its OFL license separately from the AGPL-3.0-only application code.

## Calibration workflow

Reference screenshots stay outside the repository. Render a fabricated fixture to PDF, rasterise the page, and compare it with a separately supplied cropped reference:

```bash
python scripts/calibrate.py /private/reference-page.png /tmp/ttyinv-page.png \
  --out-dir /tmp/ttyinv-calibration
```

The script produces an overlay, a contrast-enhanced difference image, and a side-by-side view. Calibration targets the outer frame, page insets, header and party separators, column rhythm, baseline positions, font size, line height, and theme tokens. Glyph-level comparison is meaningful only when Geist Mono has been vendored.
