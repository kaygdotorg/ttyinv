---
schema: ttyinv/v1
invoice:
  number: INV-2026-103
  title: Engineering services and travel
  issued: 2026-03-20
  due: 2026-04-03
  currency: EUR
  locale: en-GB
from:
  name: Atlas Systems Cooperative
  address:
    - 5 Demonstration Lane
    - Dublin
    - Ireland
  identifiers:
    VAT: IE0000000X
  email: finance@example.com
to:
  name: Sample Observatory SAS
  address:
    - 17 Example Quai
    - Lyon
    - France
  identifiers:
    VAT: FR00000000001
payment:
  title: Payment
  methods:
    - title: Transfer
      fields:
        Beneficiary: Atlas Systems Cooperative
        Reference: INV-2026-103
        Instructions: Contact finance@example.com for remittance details
---

## Engineering fees

| Description | Days | Rate | Amount (EUR) |
| --- | ---: | ---: | ---: |
| Reliability engineering<br>3 Mar 2026 to 14 Mar 2026 | 10 | 720.00 | auto |

## Travel expenses

| Description | Amount (JPY) | Amount (EUR) |
| --- | ---: | ---: |
| Airport rail | 4200 | 25.90 |
| Hotel transfer | 6800 | 41.93 |
| Working dinner | 9100 | 56.11 |

## Notes

Source-currency amounts are informational. The explicitly supplied EUR column is payable and no live exchange-rate lookup is performed.
