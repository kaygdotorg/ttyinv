---
schema: ttyinv/v1
invoice:
  number: INV-2026-105
  title: Multi-page service report
  issued: 2026-05-28
  due: 2026-06-11
  currency: EUR
  locale: en-GB
from:
  name: Longform Technical Studio
  address:
    - 3 Fabricated Crescent
    - Brussels
    - Belgium
  identifiers:
    VAT: BE0000000000
  email: billing@example.com
to:
  name: Demonstration Infrastructure BV
  address:
    - 90 Example Canal
    - Rotterdam
    - Netherlands
  identifiers:
    VAT: NL000000000B00
payment:
  title: Payment
  methods:
    - title: Transfer
      fields:
        Beneficiary: Longform Technical Studio
        Reference: INV-2026-105
        Instructions: Contact billing@example.com for transfer details
---

## Implementation work

| Description | Hours | Rate | Amount (EUR) |
| --- | ---: | ---: | ---: |
| Architecture session 01 | 2 | 180.00 | auto |
| Architecture session 02 | 2 | 180.00 | auto |
| Architecture session 03 | 2 | 180.00 | auto |
| Architecture session 04 | 2 | 180.00 | auto |
| Architecture session 05 | 2 | 180.00 | auto |
| Architecture session 06 | 2 | 180.00 | auto |
| Architecture session 07 | 2 | 180.00 | auto |
| Architecture session 08 | 2 | 180.00 | auto |
| Architecture session 09 | 2 | 180.00 | auto |
| Architecture session 10 | 2 | 180.00 | auto |
| Implementation batch 01 | 4 | 180.00 | auto |
| Implementation batch 02 | 4 | 180.00 | auto |
| Implementation batch 03 | 4 | 180.00 | auto |
| Implementation batch 04 | 4 | 180.00 | auto |
| Implementation batch 05 | 4 | 180.00 | auto |
| Implementation batch 06 | 4 | 180.00 | auto |
| Implementation batch 07 | 4 | 180.00 | auto |
| Implementation batch 08 | 4 | 180.00 | auto |
| Implementation batch 09 | 4 | 180.00 | auto |
| Implementation batch 10 | 4 | 180.00 | auto |
| Verification pass 01 | 3 | 180.00 | auto |
| Verification pass 02 | 3 | 180.00 | auto |
| Verification pass 03 | 3 | 180.00 | auto |
| Verification pass 04 | 3 | 180.00 | auto |
| Verification pass 05 | 3 | 180.00 | auto |
| Documentation chapter 01 | 2 | 180.00 | auto |
| Documentation chapter 02 | 2 | 180.00 | auto |
| Documentation chapter 03 | 2 | 180.00 | auto |
| Documentation chapter 04 | 2 | 180.00 | auto |
| Documentation chapter 05 | 2 | 180.00 | auto |
| Handover workshop 01 | 2 | 180.00 | auto |
| Handover workshop 02 | 2 | 180.00 | auto |

## Notes

This deliberately long fixture verifies repeated table headings, unsplit rows, totals kept together, continuation-page framing, and a closing Payment block that is allowed to move as a unit.
