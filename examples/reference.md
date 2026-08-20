---
schema: ttyinv/v1
invoice:
  number: EXAMPLE-2026-0417
  title: Invoice
  issued: 12 Jun 2026
  due: 26 Jun 2026
  terms: Net 14
  currency: EUR
  locale: en-GB
from:
  name: Terminal Works
  logo: ./assets/example-mark.svg
  address:
    - 15 Sample Avenue
    - 75016 Paris
    - France
  identifiers:
    VAT: FR00000000000
to:
  name: Example Automotive S.p.A.
  address:
    - 4 Prototype Way
    - 41053 Maranello
    - Italy
  identifiers:
    VAT: IT00000000000
payment:
  title: Payment
  methods:
    - title: Bank transfer
      fields:
        Beneficiary: Terminal Works
        IBAN: FR00 0000 0000 0000 0000 000
        BIC: EXAMPLE0XXX
        Bank: Example Bank
signature:
  image: ./assets/example-signature.svg
  name: Avery Example
  label: Authorised signature
---

## Contract fees

| Description | Qty | Unit price | Code | Amount (EUR) |
| :--- | ---: | ---: | :--- | ---: |
| Platform subscription<br>112 seats, 1 Jun 2026 to 30 Jun 2026 | 112 | 21.04 | CORE | auto |
| Connector sync - metered<br>48,200 records ingested above the plan allowance | 48.2 | 3.50 | MTR | auto |
| Onboarding and data migration<br>One-off implementation, delivered 4 Jun 2026 | 1 | 1450.00 | SETUP | auto |

## Notes

Payment is due within fourteen days.
