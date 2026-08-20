---
schema: ttyinv/v1
invoice:
  number: EXAMPLE-2026-005
  title: Invoice
  issued: 20 Sep 2026
  due: 4 Oct 2026
  currency: EUR
  locale: en-GB
from:
  name: Northstar Studio
  address: [10 Example Street, 75001 Paris, France]
  identifiers: { VAT: FR00000000000 }
to:
  name: Example Customer GmbH
  address: [20 Sample Road, 10115 Berlin, Germany]
  identifiers: { VAT: DE000000000 }
payment:
  title: Payment
  methods:
    - title: Bank transfer
      fields:
        Beneficiary: Northstar Studio
        Reference: EXAMPLE-2026-005
settlements:
  - date: 28 Sep 2026
    paid: { amount: 5132.70, currency: EUR }
    received: { amount: 478000.00, currency: INR }
---

## Contract fees

| Description | Days | Rate | Amount (EUR) |
| :--- | ---: | ---: | ---: |
| Systems consulting<br>1 Sep 2026 to 30 Sep 2026 | 20 | 250.00 | auto |

## Operating expenses

| Description | Amount (KZT) | Amount (EUR) |
| :--- | ---: | ---: |
| Airport meal | 7100.00 | 12.70 |
| Ground transport | 2800.00 | 5.40 |

## Notes

Source-currency conversions are explicitly supplied and are not looked up by `ttyinv`.
