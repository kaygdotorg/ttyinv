# ttyinv

`ttyinv` is a local-only CLI that renders a strict Markdown invoice as print-ready A4 HTML and/or PDF.

```text
invoice.md -> parse -> validate -> calculate -> HTML -> Chromium -> PDF
```

It is intentionally only an invoice renderer. There is no web app, editor, database, account, invoice history, tax engine, exchange-rate lookup, telemetry, or network service.

## Status

This repository contains the calibrated `0.1.6` implementation of the `ttyinv/v1` dialect.

- Light is the default, print-friendly theme.
- Dark uses the same document geometry and pagination.
- HTML and PDF are generated from the same renderer.
- Geist Mono is the canonical typeface; release builds embed the regular and semibold webfonts.
- The full dashed A4 page frame is always present; its corner `+` junctions are CSS strokes, not font glyphs.
- Financial tables remain borderless, while their header/body rules and total rule share one column grid.
- Financial and prose section labels use the same left-inset bracket treatment as the Payment block.
- The HTML output contains inline CSS and embeds configured logos, signatures, Markdown images, and fonts.

The visual system is calibrated against the supplied reference while keeping the light theme canonical for print.

## Requirements

- Python 3.11 or newer.
- Chromium or Google Chrome for PDF output.

## Install from source

```bash
python -m venv .venv
source .venv/bin/activate
pip install -e .
```

For development:

```bash
pip install -e '.[dev]'
make check
```

For deterministic release builds, vendor Geist Mono from its official tagged source before building the wheel:

```bash
make release
```

The release target vendors the fonts, verifies that both weights and the license are present, and then builds the distributions. The vendor command also stores the font's `OFL.txt` license beside the packaged webfonts. `ttyinv` never fetches fonts while rendering an invoice.

## Usage

```bash
# Light A4 PDF beside invoice.md
ttyinv invoice.md

# One self-contained HTML file
ttyinv invoice.md --format html

# HTML and PDF
ttyinv invoice.md --format both

# Dark output
ttyinv invoice.md --theme dark

# Override the accent (any safe CSS color)
ttyinv invoice.md --accent "#ff5c5c"

# Use an installed font that ttyinv verifies as monospace
ttyinv --list-fonts
ttyinv invoice.md --font "Maple Mono NF"

# Choose an output filename or stem
ttyinv invoice.md --output ./out/invoice

# Explicit amount policies
ttyinv invoice.md --trust-explicit
ttyinv invoice.md --recalculate

# Select Chromium explicitly
ttyinv invoice.md --chromium /usr/bin/chromium
```

Chromium can also be selected with `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH` or `CHROME_PATH`.

## Minimal invoice

```md
---
schema: ttyinv/v1
invoice:
  number: INV-2026-004
  title: Invoice
  issued: 20 Aug 2026
  due: 3 Sep 2026
  terms: Net 14
  currency: EUR
  locale: en-GB
from:
  name: Example Studio
to:
  name: Example Client
---

## Contract fees

| Description | Days | Rate | Amount (EUR) |
| :--- | ---: | ---: | ---: |
| Systems engineering<br>August 2026 | 20 | 250.00 | auto |
```

The level-two heading is rendered as a left-indented bracket label on the section rule, matching the Payment label. The Markdown table header is preserved as written.

See [SPEC.md](./SPEC.md) for the complete contract.

## Amount policy

- A blank amount or `auto` is calculated from quantity times rate.
- An explicit amount that matches the calculation is accepted.
- An explicit amount that differs is an error by default.
- `--trust-explicit` keeps the written value.
- `--recalculate` replaces a written value whenever quantity and rate are available.
- An explicit amount without enough inputs to recalculate remains explicit.

`--trust-explicit` and `--recalculate` are mutually exclusive.

## Multi-currency data

An invoice has one payable currency. A table can also contain source-currency amounts:

```md
## Operating expenses

| Description | Amount (KZT) | Amount (EUR) |
| :--- | ---: | ---: |
| Airport meal | 7100.00 | 12.70 |
```

The payable-currency value must be supplied explicitly. `ttyinv` does not fetch exchange rates.

## Assets and links

All relative paths are resolved from the source Markdown file.

- Local logos, signatures, Markdown images, and configured fonts are embedded as data URLs.
- HTTP, HTTPS, email, and anchor links remain clickable.
- Relative document links are rebased for HTML.
- PDF local-file links use `file:` URLs and are best effort because viewers apply different security policies.
- Missing local targets produce a warning.

System font override:

```bash
ttyinv --list-fonts
ttyinv invoice.md --font "JetBrains Mono"
```

`--font` accepts only installed families that ttyinv verifies as Latin monospace. The chosen regular and strong faces are embedded into the output so the generated HTML/PDF no longer depends on that system font.

Optional embedded font configuration:

```yaml
appearance:
  font:
    family: Invoice Mono
    regular: ./assets/InvoiceMono-Regular.woff2
    bold: ./assets/InvoiceMono-Bold.woff2
```

Without an invoice-specific font configuration, the renderer uses bundled Geist Mono. A source checkout that has not run `make fonts` first tries a locally installed Geist Mono and then falls back to the system monospace stack, emitting a warning. Rendering never makes a font request over the network.

## Privacy boundary

Real invoice inputs and generated outputs often contain addresses, tax identifiers, bank details, and signatures.

- Never add real invoice PDFs, screenshots, signatures, or Markdown inputs to the public repository.
- Public fixtures must be fabricated.
- `make privacy` runs a repository privacy gate.
- Generated `.html` and `.pdf` files are ignored by default.

See [PRIVACY.md](./PRIVACY.md).

## License

AGPL-3.0-only. See [LICENSE](./LICENSE).
