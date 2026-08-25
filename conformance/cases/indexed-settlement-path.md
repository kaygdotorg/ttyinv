---
schema: ttyinv/v1
invoice:
  number: CONF-INDEX
  issued: 2026-01-15
  currency: EUR
from:
  name: Example Sender
to:
  name: Example Recipient
settlements:
  - date: 2026-01-18
    paid:
      amount: 10.00
      currency: EUR
  - date: 2026-01-19
    paid:
      amount: 20.00
      currency: EUR
  - date: 2026-02-30
    paid:
      amount: 30.00
      currency: EUR
---

## Services

| Description | Amount (EUR) |
| --- | ---: |
| Consulting | 100.00 |
