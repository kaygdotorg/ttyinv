---
schema: ttyinv/v1
invoice:
  number: EXAMPLE-2026-004
  title: Invoice
  issued: 20 Aug 2026
  due: 3 Sep 2026
  terms: Net 14
  currency: EUR
  locale: en-GB
from:
  name: Northstar Studio
  logo: ./assets/example-mark.svg
  address:
    - 10 Example Street
    - 75001 Paris
    - France
  identifiers:
    VAT: FR00000000000
  email: billing@example.com
to:
  name: Example Customer GmbH
  address:
    - 20 Sample Road
    - 10115 Berlin
    - Germany
  identifiers:
    VAT: DE000000000
payment:
  title: Payment
  methods:
    - title: Bank transfer
      fields:
        Beneficiary: Northstar Studio
        IBAN: DE00 0000 0000 0000 0000 00
        BIC: EXAMPLE0XXX
        Bank: Example Bank
signature:
  image: ./assets/example-signature.svg
  name: Avery Example
  label: Authorised signature
settlements: []
---

## Contract fees

| Description | Days | Rate | Amount (EUR) |
| :--- | ---: | ---: | ---: |
| Systems consulting<br>1 Aug 2026 to 31 Aug 2026 | 20 | 250.00 | auto |

## Notes

Payment is due within fourteen days. See the [previous invoice](./previous-invoice.pdf).
