---
schema: ttyinv/v1
invoice:
  number: TTY-2026-0417
  title: Platform services
  issued: 2026-06-12
  due: 2026-06-26
  currency: EUR
  locale: en-GB
  terms: Net 14
from:
  name: Terminal Works SAS
  address:
    - 15 Example Rue
    - 75016 Paris
    - France
  identifiers:
    VAT: FR00000000002
  email: billing@example.com
to:
  name: Demonstration Motors S.p.A.
  address:
    - 4 Prototype Via
    - 41053 Maranello
    - Italy
  identifiers:
    VAT: IT00000000000
payment:
  title: Payment
  methods:
    - title: Transfer
      fields:
        Beneficiary: Terminal Works SAS
        Reference: TTY-2026-0417
        Instructions: Contact billing@example.com for remittance details
---

## Platform services

| Description | Qty | Unit price | Code | Amount (EUR) |
| --- | ---: | ---: | ---: | ---: |
| Platform team<br>112 seats, 1 Jun 2026 to 30 Jun 2026 | 112 | 21.04 | TEAM | auto |
| Connector sync<br>48,200 records above the plan allowance | 48.2 | 3.50 | METERED | auto |
| Onboarding and data migration<br>One-off implementation, delivered 4 Jun 2026 | 1 | 1450.00 | SETUP | auto |

## Notes

Thank you. Please include the invoice reference with payment.
