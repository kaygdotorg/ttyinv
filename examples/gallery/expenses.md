---
schema: ttyinv/v1
invoice:
  number: INV-2026-102
  title: Design and reimbursable expenses
  issued: 2026-02-18
  due: 2026-03-04
  currency: EUR
  locale: en-GB
from:
  name: Meridian Works
  address:
    - 42 Fictional Avenue
    - Lisbon
    - Portugal
  identifiers:
    VAT: PT000000000
  email: accounts@example.com
to:
  name: Example Robotics GmbH
  address:
    - 8 Prototype Platz
    - Hamburg
    - Germany
  identifiers:
    VAT: DE000000001
payment:
  title: Payment
  methods:
    - title: Transfer
      fields:
        Beneficiary: Meridian Works
        Reference: INV-2026-102
        Instructions: Request remittance details from accounts@example.com
---

## Contract fees

| Description | Hours | Rate | Amount (EUR) |
| --- | ---: | ---: | ---: |
| Product interface design | 36 | 145.00 | auto |
| Design review | 4 | 145.00 | auto |

## Operating expenses

| Description | Amount (EUR) |
| --- | ---: |
| Prototype materials | 184.30 |
| Rail travel | 96.40 |
| Accessibility audit tool | 72.00 |

## Notes

Receipts are retained by the issuer and can be supplied on request.
