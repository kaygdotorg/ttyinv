# ttyinv

`ttyinv` is a Rust invoice engine and CLI. It uses the content-first `ttyinv/v2`
Markdown grammar in [SPEC.md](SPEC.md).

## Build

```console
make test
make rust-release
```

The workspace contains:

- `ttyinv-core`: typed `Document`, Markdown parser, canonical serializer, JSON/YAML
  adapters, validation, and versioned edit operations;
- `ttyinv-cli`: `validate`, `convert`, `schema`, `sections`, and `edit` commands;
- `ttyinv-wasm`: browser exports for validation, structure manifests, revisions, and
  edit operations.

## CLI

```console
ttyinv validate invoice.md
ttyinv validate invoice.json --json
ttyinv convert invoice.md --to json --output invoice.json
ttyinv convert invoice.json --to markdown --stdout
ttyinv schema --output ttyinv-v2.schema.json
ttyinv sections invoice.md --json
ttyinv edit move-section invoice.md --from 3 --to 1 --stdout
ttyinv edit set-gap invoice.md --section 2 --gap roomy --check
ttyinv edit set-scalar invoice.md --path metadata.terms --value "Net 30" --stdout
```

CLI section positions are one based. Core edit operations use zero-based ordinary
section indices. In-place edits use a sibling temporary file and rename.

`validate` and `convert` infer input from `.md`, `.json`, `.yaml`, and `.yml`.
Use `--from` for stdin or another extension. Conversion writes canonical Markdown;
JSON and YAML use the same typed `Document` model. Conversion defaults to stdout,
while `--output` uses an atomic replacement. `edit` defaults to an atomic
in-place replacement; `--stdout`, `--check`, and `--json` never modify the input.

## Format

Every document has minimal frontmatter, an H1 title, metadata, `From` and `Bill to`
blocks, ordinary H2 sections, and optional fixed footer blocks. Ordinary sections use
one table or prose body. Immediately preceding `ttyinv` directives move with ordinary
sections.

Configuration IDs are explicit and stable. The default theme is `printable`. The default
font is `geist-mono`, with base `font-weight` defaulting to `regular` (`regular` or
`semibold`). Optional `accent` uses strict lowercase `#rrggbb` syntax; when absent,
adapters use the theme accent. `font-scale` defaults to `100` and accepts integer
percentages from `100` through `140`. `frame-inset` defaults to `54` and accepts integer
layout units from `30` through `60`. The UI may use five-point font-scale steps. These
fields configure adapters; headings and strong text remain semibold. See [SPEC.md](SPEC.md)
for all supported IDs and grammar rules.

## Canonical example

See [examples/simple.md](examples/simple.md) and the v2 conformance cases.

## License

The engine is licensed under AGPL-3.0-only. Font files in `assets/fonts` retain their
OFL license.
