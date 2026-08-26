# ttyinv/v2 specification

Status: **v2**

This specification defines the content-first Markdown format. The Rust core is the one
parser and source editor. Adapters use the same typed `Document` model.

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
font-scale: 100
frame-inset: 54
---

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
- `font-scale` defaults to `100` and accepts integer percentages from `100` to `140`.
- `frame-inset` defaults to `54` and accepts integer layout units from `30` to `60`.

The UI may offer font-scale steps of five, but parsers accept every integer in range.
`font-weight` controls the base text weight. Semantic headings and strong text remain
semibold.
These values configure adapters only; the core model does not define renderer geometry.

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
code, lists, and line breaks. Nested H2 headings are not allowed. Fenced code is prose;
headings and directives inside fences are not grammar. Empty or prose-only documents
are valid after the required fixed parties.

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

```json
{"source":"...","base_revision":"...","sequence":7,
 "operation":{"kind":"move_section","from":3,"to":1}}
```

The typed union is `SetScalar { path, value }`, `MoveSection { from, to }`, or
`SetSectionGap { section, gap }`. Paths are dotted strings. Section indices are zero
based over ordinary sections only. One drop is one operation. A revision mismatch returns
`conflict: true` and diagnostic `CONFLICT001`. Limits and decoded request bounds apply.

`SetScalar` changes visible scalar fields without rebuilding unrelated source bytes.
Metadata paths search only the list between the H1 title and the `From` block.
`sections[n].prose` requires a prose target and one unstructured, single-paragraph value.
The value cannot contain blank lines, tables, headings, lists, fences, or directives.
The edit range ends before the next block's directives.
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
