# ttyinv design contract

`ttyinv` uses a restrained terminal/Markdown visual language: monospaced typography, bracketed labels, dashed page framing, strong column rhythm, and one accent. The renderer is not a literal terminal screenshot; CSS geometry is used where font glyphs would introduce alignment drift.

## Canonical geometry

- Page size: A4.
- Light is the default print theme; dark shares the same measurements.
- The content frame is one dashed rectangular border.
- Each corner is a literal `+` glyph whose CSS box is centered on the corresponding border intersection.
- Frame placement is fixed; the corner glyphs use the same verified monospace font as the invoice.
- Section labels such as `[ Contract fees ]`, `[ Notes ]`, and `[ Payment ]` share one left inset and cut through their horizontal rule.

## Tables

Tables intentionally have no outer box and no vertical cell borders. Alignment is produced by:

- a deterministic semantic `<colgroup>`;
- right-aligned tabular numerals;
- one shared table/header/body grid;
- a final payable amount column flush with the content edge;
- table-ending, subtotal, and total-due rules derived from that grid rather than independently positioned.

A change that makes a single screenshot look better but breaks these relationships is a regression.

## Typography

Geist Mono is the canonical default. The user may select another installed font only when font-table inspection verifies printable ASCII has a fixed advance width. The selected font is embedded into self-contained output.

Monospace does not imply identical vertical metrics. Font tests record units per em, advance width, ascent, descent, line gap, and missing printable ASCII. The calibrated corner contract centers each `+` glyph box on the frame intersection and must remain valid for every supported font.

## Themes

Theme changes are tokens, not separate layouts:

```text
paper
ink
muted
rule
accent
```

`--paper`, `--ink`, `--muted`, and `--accent` are safe color overrides. `--density compact` adjusts type scale and leading but not page size, financial meaning, or outer-frame placement.

## Pagination

The renderer should repeat table headings, avoid splitting rows, keep section labels with the start of their table, keep totals together, and keep Payment/signature blocks together where practical. Content moves to another page rather than shrinking below the design's intended readability.

## Regression testing

The public repository does not store the private inspiration screenshot. `scripts/visual_contract.py` asserts relational geometry instead:

- A4 page ratio;
- one page-frame border and four centered `+` junctions;
- one section-label inset;
- table heading/body right-edge agreement;
- total alignment with the final amount column.

CI also uploads a rendered fabricated fixture for human review.
