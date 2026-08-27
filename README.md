# ttyinv

`ttyinv` is a Rust invoice engine and CLI. It uses the content-first `ttyinv/v2`
Markdown grammar in [SPEC.md](SPEC.md).

## Build

```console
make test
make rust-release
```

The workspace contains:

- `ttyinv-core`: the typed command executor, canonical `Document` model, Markdown/JSON/YAML
  decoding, validation, editing, rendering, and registry metadata;
- `ttyinv-cli`: a thin filesystem and terminal adapter over the single `execute` seam;
- `ttyinv-wasm`: one bounded `execute` export for every command and outcome.

## CLI

All domain operations enter the same core `execute` command seam. The CLI keeps only
filesystem, stdin/stdout, atomic output, and argument parsing at its boundary.

```console
ttyinv create draft.json --stdout
ttyinv validate invoice.md --json
ttyinv inspect invoice.md --mode summary --json
ttyinv convert invoice.md --to json --stdout
ttyinv edit set-scalar invoice.md --path metadata.terms --value "Net 30" --stdout
ttyinv prepare-render invoice.md --format html --output invoice.plan.json
ttyinv render invoice.md --format html --stdout
ttyinv resolve-presentation
ttyinv registry
ttyinv schema --output ttyinv-v2.schema.json
```

CLI section positions are one based. Core edit operations use zero-based ordinary
section indices. In-place edits use a sibling temporary file and rename.

`validate`, `inspect`, and `convert` infer Markdown for extensionless files and infer
JSON/YAML from `.json`, `.yaml`, and `.yml`. Use `--from` for stdin or another
extension. Conversion emits the requested canonical Markdown, JSON, or YAML format.
It defaults to stdout; `--output` uses an atomic replacement. `edit` defaults to an
atomic in-place replacement for Markdown; structured input requires `--stdout` or
`--json`, while `--check` never modifies the input.

`prepare-render` emits a serializable plan for preview and inspection only. Plans are
never accepted as `render` input; rendering always takes a typed source and options.

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

See [examples/simple.md](examples/simple.md) and the render-compat corpus.

## License

The engine is licensed under AGPL-3.0-only. Font files in `assets/fonts` retain their
OFL license.
