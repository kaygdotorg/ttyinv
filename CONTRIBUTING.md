# Contributing to ttyinv

Thank you for helping improve a deliberately small invoice renderer.

## Branch policy

Development happens on `dev`. New work should branch from `dev` when a short-lived review branch is genuinely needed, and it must return to `dev` before release.

`main` contains stable code only. Promote `dev` to `main` only after the complete test, privacy, and visual-contract gates pass and the release candidate has been inspected. Do not develop directly on `main`.

The long-lived branch model is therefore:

```text
dev  -> all development and integration
main -> stable/releasable code only
```

## Product boundary

The core `ttyinv` product is a CLI that turns one strict Markdown invoice into self-contained HTML and/or A4 PDF. The separate hosted editor at https://app.ttyinv.com provides local-first authoring and preview. Neither the CLI nor the hosted service is an invoice database, tax adviser, customer-management system, email-delivery service, or exchange-rate service; hosted account and payment records unlock export but never contain invoice content.

## Development setup

```console
make install
make check
```

Run the browser-backed geometry check before changing renderer HTML, CSS, pagination, fonts, table layout, totals, page borders, or section labels:

```console
make visual
```

Continuous integration runs the unit tests on Python 3.11, 3.12, and 3.13 only. Run every other gate on your machine before you push:

```console
make preflight
```

`make preflight` runs `make check`, `make secrets`, and `make visual`. `make secrets` scans the whole git history with Gitleaks and needs Podman or Docker. Use `make secrets CONTAINER=docker` for Docker.

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
