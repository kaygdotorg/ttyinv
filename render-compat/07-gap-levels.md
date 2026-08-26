---
schema: ttyinv/v2
format: code-comma-dot
theme: printable
font: geist-mono
density: comfortable
---

# Gap levels

- Number: INV-2026-107
- Kind: standard
- Issued: 2026-01-15
- Due: 2026-01-29
- Terms: Net 14
- Currency: EUR

## From

- Name: Fictional Studio
- Address: 1 Example Street
- Address: Example City
- Email: billing@example.com
- Website: https://studio.example
- ID.VAT: EX000000000

## Bill to

- Name: Example Client Ltd
- Address: 2 Sample Road
- Address: Sample City

<!-- ttyinv:gap-before none -->
## No gap

| Description | Units | Rate | Amount (EUR) |
|---|---:|---:|---:|
| A | 1 | 10.00 | auto |

<!-- ttyinv:gap-before tight -->
## Tight gap

Text with a tight preceding gap.

<!-- ttyinv:gap-before standard -->
## Standard gap

Text with the default gap.

<!-- ttyinv:gap-before roomy -->
## Roomy gap

Text with extra separation.
