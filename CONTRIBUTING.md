# Contributing to ttyinv

Thank you for helping improve a deliberately small invoice renderer.

## Product boundary

`ttyinv` is a CLI that turns one strict Markdown invoice into self-contained HTML and/or A4 PDF. It is not an editor, web service, invoice database, tax adviser, payment processor, customer-management system, or exchange-rate service.

## Development setup

```console
make install
make check
```

Run the browser-backed geometry check before changing renderer HTML, CSS, pagination, fonts, table layout, totals, page borders, or section labels:

```console
make visual
```

## Pull requests

Keep changes focused and include tests. Renderer changes should explain which geometric relationship is being preserved or intentionally changed. The visual contract is relational: all page-frame junctions share axes with their rules; table headers, body cells, and totals share a column grid; section labels share one left inset; and light/dark themes share the same geometry.

When adding or changing a CLI option, update `README.md`, `SPEC.md`, completion/help text where applicable, and tests.

## Privacy: fabricated data only

Read `PRIVACY.md` before adding a fixture. Never submit a real invoice, customer, address, tax identifier, bank coordinate, settlement record, signature, receipt, private reference image, token, key, or production configuration. Use invented organizations and `example.com` addresses.

A useful test fixture should be *structurally realistic* without looking operationally real. Do not merely redact a private document; write a fresh fabricated one.

## Compatibility

The `ttyinv/v1` Markdown dialect is append-only within v1. Existing valid v1 documents must not silently change financial meaning. New optional fields and non-ambiguous column aliases are acceptable; removed fields, changed calculations, and reinterpretation of existing syntax require a new schema version.

## Dependencies

Prefer small, mature dependencies with clear licenses. Browser and font changes affect reproducibility and visual output, so include a rationale and update the release constraints when necessary.

## License

By contributing, you agree that your contribution is licensed under `AGPL-3.0-only` with the rest of the project.
