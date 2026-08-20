# Contributing

## Development setup

```bash
python -m venv .venv
source .venv/bin/activate
pip install -e '.[dev]'
make check
```

## Privacy is a merge requirement

Invoice bugs must be reproduced with fabricated data. Do not add real invoices, invoice screenshots, addresses, tax identifiers, account details, signatures, or customer records to issues, fixtures, tests, snapshots, or pull requests.

Run the privacy gate before committing:

```bash
python scripts/privacy_check.py
```

The repository intentionally ignores PDFs, raster images, generated HTML, and common private working directories. Synthetic SVG assets are allowed when they are clearly fabricated.

## Design work

Keep the light and dark themes geometrically identical. Visual changes should be checked in both themes and in PDF output. Private design-reference files belong under the ignored `reference/` directory and must never be committed.

## Scope

`ttyinv` is a Markdown-to-HTML/PDF renderer. Features that require accounts, a database, a web editor, sending, tax advice, or exchange-rate lookup are outside the v1 scope.
