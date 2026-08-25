from __future__ import annotations

STARTER_INVOICE = """---
schema: ttyinv/v1
invoice:
  number: INV-2026-001
  title: Invoice
  issued: 2026-08-20
  due: 2026-09-03
  terms: Net 14
  currency: EUR
  locale: en-GB
from:
  name: Example Studio
  address:
    - 10 Example Street
    - Paris
    - France
  identifiers:
    VAT: FR00000000000
to:
  name: Example Client
  address:
    - 20 Sample Road
    - Berlin
    - Germany
  identifiers:
    VAT: DE000000000
payment:
  title: Payment Methods
  methods:
    - title: Bank transfer
      fields:
        Beneficiary: Example Studio
        IBAN: DE00 0000 0000 0000 0000 00
        BIC: EXAMPLE0XXX
        Bank: Example Bank
---

## Contract fees

| Description | Days | Rate | Amount (EUR) |
| :--- | ---: | ---: | ---: |
| Systems consulting<br>August 2026 | 20 | 250.00 | auto |

## Notes

Payment is due within fourteen days.
"""

STARTER_LOGO_SVG = """<svg xmlns="http://www.w3.org/2000/svg" width="160" height="48" viewBox="0 0 160 48" role="img" aria-labelledby="title">
  <title id="title">Fabricated Example Studio mark</title>
  <rect width="160" height="48" rx="4" fill="#1268a8"/>
  <text x="80" y="30" text-anchor="middle" fill="white" font-family="monospace" font-size="16">EXAMPLE STUDIO</text>
</svg>
"""

STARTER_SIGNATURE_SVG = """<svg xmlns="http://www.w3.org/2000/svg" width="240" height="72" viewBox="0 0 240 72" role="img" aria-labelledby="title">
  <title id="title">Fabricated example signature</title>
  <path d="M12 51 C45 12 54 64 82 32 S126 62 151 29 S188 57 226 20" fill="none" stroke="#1268a8" stroke-width="4" stroke-linecap="round"/>
</svg>
"""


def starter_invoice(*, with_assets: bool = False) -> str:
    """Return the static fabricated starter, optionally referencing fabricated SVGs."""

    if not with_assets:
        return STARTER_INVOICE
    return STARTER_INVOICE.replace(
        "  identifiers:\n    VAT: FR00000000000\n",
        "  identifiers:\n    VAT: FR00000000000\n  logo: ./assets/logo.svg\n",
    ).replace(
        "payment:\n",
        "signature:\n  image: ./assets/signature.svg\n  name: Example Signatory\n  label: Fabricated signature\npayment:\n",
    )
