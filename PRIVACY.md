# Privacy policy for repository content

`ttyinv` is an invoice renderer, so realistic-looking examples can accidentally expose unusually sensitive data. This repository follows a strict rule: **only fabricated invoice data may be committed.**

## Never commit

- real invoices or invoice exports;
- real personal or company names used in a private invoice;
- postal addresses, telephone numbers, customer records, or personal email addresses;
- PAN, GSTIN, VAT, tax, registration, or government identifiers;
- IBANs, account numbers, routing numbers, SWIFT/BIC values, cards, payment links, or settlement records;
- handwritten or digital signatures;
- private reference screenshots, logos, contracts, receipts, expense evidence, or supporting documents;
- API tokens, private keys, credentials, cookies, environment files, or CI secrets;
- downloaded font binaries unless their inclusion and license have been explicitly reviewed.

## Public examples

All examples use invented organizations and `example.com` addresses. Identifier-shaped values are obvious placeholders such as `FR00000000000`; payment instructions never contain usable account coordinates. SVG marks and signatures in the repository are synthetic artwork, not traced or copied from a real invoice.

## Controls

The repository uses several independent controls:

1. `.gitignore` excludes common generated invoice files, private workspaces, raster images, font binaries, and credential formats.
2. `scripts/privacy_check.py` scans tracked source for high-risk file types and identifier patterns.
3. Gitleaks scans history and proposed changes in CI.
4. Fabricated examples are linted and rendered in CI; private invoices are never required for tests.
5. Release automation runs the privacy gate before building artifacts.

Automated scanning is defense in depth, not permission to commit realistic private data. Contributors must inspect their staged diff before every push.

## Working with private invoices

Keep private Markdown, assets, signatures, and generated output outside the repository. A recommended layout is:

```text
~/invoices-private/
  invoice.md
  assets/
~/src/ttyinv/
```

Run the installed CLI from any directory; paths inside an invoice are resolved relative to that invoice file.

## Accidental disclosure

Stop immediately. Do not attempt to hide the data with a follow-up commit. Revoke or rotate exposed credentials and payment details, remove the material from Git history, and assess whether cached release artifacts or forks also require cleanup.
