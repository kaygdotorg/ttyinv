from __future__ import annotations

STARTER_INVOICE = """---
schema: ttyinv/v1
invoice:
  number: INV-2026-001
  title: Invoice
  issued: 20 Aug 2026
  due: 3 Sep 2026
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
