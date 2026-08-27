# ttyinv/v2 specification

Status: **v2**

This specification defines the content-first Markdown format and its shared command
executor. The Rust core exposes one domain seam:

```rust
pub fn execute(command: InvoiceCommand<'_>) -> Result<CommandOutcome, CommandError>;
```

CLI and WASM perform only transport, filesystem, and bounded-input work around `execute`.

## Document grammar

A document is UTF-8 text with one frontmatter mapping, one H1 title, metadata, fixed
party blocks, zero or more ordinary H2 sections, and optional fixed footer blocks.

```md
---
schema: ttyinv/v2
format: code-comma-dot
theme: printable
font: geist-mono
font-weight: regular
density: comfortable
accent: "#2f6fed"
amount-in-words: true
font-scale: 100
frame-inset: 54

# Consulting services

- Number: INV-2026-001
- Kind: standard
- Issued: 2026-01-15
- Due: 2026-01-29
- Terms: Net 14
- Currency: EUR

## From
- Name: Northstar Studio

## Bill to
- Name: Acme Research Ltd

## Contract fees
| Description | Days | Rate | Amount (EUR) |
|---|---:|---:|---:|
| Systems consulting | 8 | 650.00 | auto |
```

The source uses exact English labels and exact reserved headings. Labels are case
sensitive. Blank lines are allowed between grammar elements.

## Frontmatter configuration

Frontmatter contains only these keys:

- `schema` is required and equals `ttyinv/v2`.
- `format` defaults to `code-comma-dot`.
- `theme` defaults to `printable`.
- `font` defaults to `geist-mono`.
- `font-weight` defaults to `regular` and accepts `regular` or `semibold`.
- `density` defaults to `comfortable`.
- `accent` is optional. It must be lowercase `#rrggbb`; absent uses the theme accent.
- `amount-in-words` defaults to `false`; when true, it enables engine-generated amount words.
- `font-scale` defaults to `100` and accepts integer percentages from `100` to `140`.
- `frame-inset` defaults to `54` and accepts integer A4 layout units from `30` to `60`. The content gutters are 11.57 units horizontally, 23.14 units above, and 17.35 units below.

`amount-in-words` defaults to `false`. When `true`, the engine generates words for
ordinary table subtotals, the grand total, and each settlement `Received` amount.
Adapters MUST use the generated values in `PreparedRender.amount_in_words`; they
MUST NOT generate or alter the words. Amounts use title case, hyphenated compound
numbers, and end with `Only`. International grouping applies to every currency
except INR, which uses Indian thousands, lakh, and crore grouping. Currency minor
units use the ISO currency convention: EUR and USD use cents, INR uses paise, JPY
has no minor unit, and KWD uses fils. Amounts are rounded to the currency exponent.
Zero is rendered as `Zero ... Only`; negative non-zero values begin with `Negative`.

The UI may offer font-scale steps of five, but parsers accept every integer in range.
`font-weight` controls the base text weight. Semantic headings and strong text remain
semibold. The core renderer resolves these values into one immutable layout plan.

Rendering is an executor command. `PrepareRender` returns a versioned, serializable,
source-free `PreparedRender` plan for preview and inspection. Plans are never accepted
as `Render` input. `Render` always accepts the original typed source and options, then
prepares and encodes through the same bounded pipeline for HTML, PDF, or PNG. Render
outcomes include the source revision, plan digest, dimensions, warnings, output bytes,
and output hash.
- `png-scale` is a render option for PNG output. It accepts `1` or `2`; omitted defaults
  to `1`. Scale doubles raster dimensions without changing logical A4 geometry.
Rendering never fetches external assets. Images must be bounded asset bytes supplied by
the adapter and validated by the executor. Invalid documents, options, assets, and
oversize output return typed `CommandError` values.
All floating-point fields in `Presentation` and `PreparedRender` use one canonical
wire representation: the renderer computes an IEEE-754 binary32 value, then
promotes that value exactly to IEEE-754 binary64 at the serialization boundary.
The resulting JSON number uses one shortest spelling (integral values are emitted
without a trailing `.0`; for example, binary32 `770.65` serializes as
`770.6500244140625`) and is emitted identically by every adapter. This promotion
preserves layout fidelity and is part of the wire contract; adapters MUST NOT
round, truncate, or otherwise normalize these numbers. `PreparedRender.plan_digest`
is computed over this canonical serialization.

## Command-envelope transport

The CLI exposes the public executor directly with `ttyinv execute`. It reads one JSON
`InvoiceCommand` envelope from stdin, or from `--input FILE`, calls `execute` once, and
writes one JSON `CommandOutcome` to stdout. Executor failures write the serialized
`CommandError` to stdout and use the established CLI exit-code mapping. Malformed JSON,
unknown command kinds, and unknown envelope fields are invalid requests.

The command envelope is limited to 256 KiB at the CLI boundary. Core source and asset
limits still apply after deserialization. In JSON, `Rendered.bytes` is an array of
unsigned octets (`0` through `255`), preserving every rendered byte without encoding
loss. This is the CLI JSON representation; adapters may expose an equivalent binary
view for the same bytes.

Formats are `code-comma-dot`, `code-dot-comma`, `code-space-comma`, `code-indian`,
and `code-plain`. Themes are `printable`, `paper-white`, `graphite`, `blueprint`,
`ledger-pad`, `solarized-light`, `parchment`, `midnight`, `nord`, and `gruvbox-dark`.
Fonts are `geist-mono`, `cousine`, `fira`, `ibm-plex`, `inconsolata`, `jetbrains`,
`roboto`, `source-code`, `space`, and `ubuntu`. Density is `comfortable` or `compact`.
Unknown keys and values are errors.

## Title and metadata

The H1 title is required. A metadata list follows it directly. The list contains each
label at most once, in any order. The canonical serializer uses this order: `Number`,
`Kind`, `Issued`, `Due`, `Terms`, `Currency`.

`Number`, `Issued`, and `Currency` are required. `Kind` defaults to `standard`. `Due`
and `Terms` are optional. Dates are real Gregorian `YYYY-MM-DD` dates. Due cannot be
before Issued. Currency is three uppercase ASCII letters.
- Rendered money values use the ISO 4217 currency code, a no-break space (`U+00A0`), and the formatted amount.

## Parties

`From` and `Bill to` are required H2 blocks. They occur in that order before ordinary
sections. Each party has a required `Name`, repeatable `Address`, optional `Email` and
`Website`, and optional identifiers. An identifier uses `ID.KEY`, where `KEY` is a safe
identifier. Each party has at most one optional Markdown image. Image alt text and the
local path or link are required. The engine never fetches an image.

```md
![Northstar Studio logo](./logo.png)

- Name: Northstar Studio
- Address: 10 Example Street
- ID.VAT: DE000000000
```

## Ordinary sections

An ordinary section is a column-zero H2 heading with exactly one body kind. A body is
one GFM table or Markdown prose. Prose supports paragraphs, emphasis, links, inline
code, lists, and hard line breaks. Every output format uses the same parsed inline
semantics; delimiters never appear as rendered content. Fenced code is prose; headings
and directives inside fences are not grammar. Empty or prose-only documents are valid
after the required fixed parties.

Reserved ordinary names are impossible. The reserved names are `From`, `Bill to`,
`Settlements`, `Payment`, and `Signature`. Ordinary sections are indexed from zero.
The source list marker is exactly `- `; `*` and `+` are not aliases.
## Directives

A directive is an exact column-zero line immediately preceding its block. Unknown,
indented, or malformed directives are errors. An ordinary block owns its directives;
moving it never captures the next block's directives. `SetSectionGap` changes only
`gap-before`; page-break and summary flags remain unchanged. The default gap is
`standard`.

```md
<!-- ttyinv:page-break-before -->
<!-- ttyinv:summary-only -->
<!-- ttyinv:gap-before none|tight|standard|roomy -->
```

## Fixed footer blocks

Footer blocks occur in this order and cannot move: optional `Settlements`, optional
`Payment`, optional `Signature`. `page-break-before` is typed in the manifest and
structured adapters for every footer block.
`Settlements` is one table with these exact headings:

```text
Date | Paid | Paid currency | Received | Received currency
```

Each date is real ISO date text. Each amount is an exact decimal without thousands
separators. Each currency is a three-letter uppercase code. GFM table pipes are escaped
as `\|`, literal backslashes as `\\`, and typed newlines serialize as `<br>`. Alignment

Adapters supply local image bytes. Native and WASM adapters accept byte arrays;
JSON, REST, and MCP transports use canonical padded base64 strings or numeric
byte sequences. The engine never fetches remote assets. Invalid base64 is rejected.
markers (`:---`, `:---:`, `---:`) are preserved in the typed table model.

`Payment` contains one or more column-zero H3 method headings. Each method contains
labelled fields in authored order. `Signature` contains at most one optional image and
required `Name` and `Label` fields.

## Images and links

Images use Markdown syntax with non-empty alt text and a source path or link. Local paths
are resolved by the adapter. The core engine does not fetch remote content. HTTP links
remain links.

## Adapters and serialization

Markdown is canonical and serializes with LF and no BOM. Its serializer emits
deterministic frontmatter, metadata order, fixed block order, directive order, and
table syntax. UTF-8 BOM and CRLF are accepted at input.
JSON and YAML parse through the same semantic validator and escape canonical Markdown
structure; injected headings, labels, directives, and table cells are rejected. Config
DTO keys use `font_weight`, `font_scale`, and `frame_inset`; Markdown frontmatter uses
`font-weight`, `font-scale`, and `frame-inset`. Core source and edit inputs are limited
to 128 KiB. The WASM decoded request limit is 256 KiB, including operation paths and
values.

Every outcome `revision` is the lowercase hexadecimal SHA-256 digest of the canonical
Markdown source. It is independent of the requested input or output representation:
converting the same document to JSON or YAML does not change its revision.

```json
{"source":"...","base_revision":"...","sequence":7,
 "operation":{"kind":"move_section","from":3,"to":1}}
```

The typed union is `SetScalar { path, value }`, `MoveSection { from, to }`, or
`SetSectionGap { section, gap }`. Paths are dotted strings. Section indices are zero
based over ordinary sections only. One drop is one operation. A revision mismatch returns
`conflict: true` and diagnostic `CONFLICT001`. Limits and decoded request bounds apply.

`SetScalar` changes visible scalar fields without rebuilding unrelated source bytes.
For `config.*` paths, an empty value removes the optional `config.accent` key so the
renderer uses the theme accent; empty values for all other configuration keys are
rejected with `EDIT004`. Unknown configuration values are rejected by validation.
Metadata paths search only the list between the H1 title and the `From` block.
`sections[n].prose` requires a prose target and one unstructured, single-paragraph value.
The value cannot contain blank lines, tables, headings, lists, fences, or directives.
`MoveSection` moves the section and all immediately preceding directives. `SetSectionGap`
updates, inserts, or removes the canonical gap directive. All operations validate the
result before returning it.

For explicit amounts, the allowed difference from quantity times rate is inclusive.
The tolerance is half the authored rate's last decimal unit times absolute quantity,
plus half the currency minor unit. Amount rounding uses decimal half-even.

## Diagnostics

Diagnostics have one stable JSON shape for CLI, WASM, and future adapters. Errors make a
document invalid. Diagnostics include a code, severity, message, and optional source and
field locations. No v1 parser or compatibility path exists.

### Diagnostic codes

The core emits the following diagnostics. Every one is an error: a document or
edit is rejected when it produces one.

| Code | Severity | Meaning |
| --- | --- | --- |
| `CONFLICT001` | error | The edit's base revision does not match the current source. |
| `CURRENCY001` | error | A currency is not three uppercase ASCII letters. |
| `CURRENCY002` | error | A settlement amount is not a decimal. |
| `DATE001` | error | A date is not a real `YYYY-MM-DD` date. |
| `DATE002` | error | Due is earlier than Issued. |
| `DIRECTIVE001` | error | A directive is unknown or indented. |
| `DIRECTIVE002` | error | A directive is misplaced or invalid for its fixed block. |
| `EDIT002` | error | A section or payment-method index is out of bounds. |
| `EDIT003` | error | An edit path is invalid or not editable. |
| `EDIT004` | error | An edit target block, field, or table cell is absent. |
| `LIMIT001` | error | Source or edit input exceeds the size limit. |
| `MARKDOWN001` | error | The document has a missing, empty, or misplaced H1/H2 heading. |
| `MONEY002` | error | An amount must be a decimal. |
| `MONEY003` | error | An `auto` amount needs a numeric quantity and rate. |
| `MONEY004` | error | An explicit amount differs from quantity × rate beyond allowed rounding. |
| `MONEY008` | error | A summary row requires an explicit amount. |
| `MONEY009` | error | A table has duplicate payable amount columns. |
| `SCHEMA001` | error | Frontmatter or metadata has an invalid shape. |
| `SCHEMA002` | error | The schema is not `ttyinv/v2`. |
| `SCHEMA003` | error | A required metadata, party, or signature label is missing. |
| `SCHEMA004` | error | A metadata label is repeated. |
| `SCHEMA005` | error | A required fixed party block is missing. |
| `SCHEMA006` | error | Fixed blocks are out of order or repeated. |
| `SCHEMA007` | error | A frontmatter configuration value is unsupported. |
| `SCHEMA008` | error | A party or signature field, image, or identifier is invalid. |
| `SCHEMA009` | error | Payment content or method fields are invalid. |
| `TABLE001` | error | A table heading or separator is missing or invalid. |
| `TABLE002` | error | A table has no body row. |
| `TABLE003` | error | A table row has the wrong number of cells. |
| `TABLE004` | error | A table contains prose or mixes table and prose content. |
| `TABLE005` | error | Settlement headings do not use the fixed names and order. |
