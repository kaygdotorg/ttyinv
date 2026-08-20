# Privacy boundary

`ttyinv` processes documents that routinely contain addresses, tax identifiers, bank
details, signatures, and other sensitive material.

The public repository must never contain real invoices or identifying data:

- Real PDFs, screenshots, scanned signatures, and source Markdown remain outside the repository.
- Public examples use invented people, companies, addresses, identifiers, and payment details.
- `private/`, `reference/`, `real-invoices/`, `*.private.md`, and `*.real.md` are ignored.
- Raster images and PDFs are ignored by default.
- `python scripts/privacy_check.py` blocks common high-risk files and identifiers.

Private reference invoices used during design are not copied into fixtures, tests,
documentation, snapshots, package data, commits, or release artifacts.
