---
schema: ttyinv/v1
invoice:
  number: INV-2026-104
  title: Research support
  issued: 2026-04-21
  due: 2026-05-05
  currency: EUR
  locale: en-GB
from:
  name: Lattice Field Office
  address:
    - 77 Placeholder Street
    - Vienna
    - Austria
  identifiers:
    VAT: ATU00000000
  email: finance@example.com
to:
  name: Fictional Materials Oy
  address:
    - 11 Sample Esplanade
    - Helsinki
    - Finland
  identifiers:
    VAT: FI00000000
settlements:
  - date: 2026-05-02
    paid:
      amount: "4680.00"
      currency: EUR
    received:
      amount: "516920.40"
      currency: INR
payment:
  title: Payment
  methods:
    - title: Remittance record
      fields:
        Beneficiary: Lattice Field Office
        Reference: INV-2026-104
        Instructions: Settlement details in this fixture are entirely fabricated
---

## Contract fees

| Description | Days | Rate | Amount (EUR) |
| --- | ---: | ---: | ---: |
| Research support<br>1 Apr 2026 to 18 Apr 2026 | 9 | 520.00 | auto |

## Notes

This fixture demonstrates an optional post-payment settlement record. All values and organizations are fictitious.
