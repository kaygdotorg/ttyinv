---
schema: ttyinv/v1
invoice:
  number: INV-2026-101
  title: Consulting services
  issued: 2026-01-15
  due: 2026-01-29
  currency: EUR
  locale: en-GB
from:
  name: Northstar Studio
  address:
    - 10 Example Street
    - Paris
    - France
  identifiers:
    VAT: FR00000000000
  email: billing@example.com
to:
  name: Acme Research Ltd
  address:
    - 20 Sample Road
    - Berlin
    - Germany
  identifiers:
    VAT: DE000000000
payment:
  title: Payment
  methods:
    - title: Bank transfer
      fields:
        Beneficiary: Northstar Studio
        Reference: INV-2026-101
        Instructions: Contact billing@example.com for transfer details
---

## Contract fees

| Description | Days | Rate | Amount (EUR) |
| --- | ---: | ---: | ---: |
| Systems consulting<br>1 Jan 2026 to 15 Jan 2026 | 8 | 650.00 | auto |
| Documentation workshop<br>Delivered remotely | 1 | 900.00 | auto |

## Notes

Payment is due within fourteen days. Project materials are available at [example.com](https://example.com/project).
