# ttyinv/v1 specification

Status: **pre-release v1 dialect**

This document defines the portable Markdown invoice format accepted by `ttyinv`. The format is narrow: YAML frontmatter carries invoice metadata; H2 Markdown headings and GFM tables carry sections; prose carries notes and links. The renderer rejects ambiguity.

## 1. Document structure

A document is UTF-8 text with:

1. one YAML frontmatter mapping delimited by `---` at the beginning of the file;
2. zero or more H2 sections;
3. GFM tables or Markdown prose within those sections.

A document with no H2 sections is valid. The validator emits warning `MARKDOWN003` because the document has no H2 section.

A document with no financial table emits warning `MARKDOWN002`. Warnings do not make the document invalid.

An authored empty `##` heading is an error under `MARKDOWN001`. An H1 heading is an error under `MARKDOWN001`. The grammar excludes both forms.

```text
---
<frontmatter>
---

## <section title>

<table or prose>
```

H1 is not part of the invoice grammar. The renderer supplies the invoice title from frontmatter.

## 2. Frontmatter

### 2.1 Required root keys

```yaml
schema: ttyinv/v1
invoice:
  number: INV-2026-001
  issued: 2026-01-15
  currency: EUR
from:
  name: Northstar Studio
to:
  name: Acme Research Ltd
```

Unknown root keys are errors unless a later compatible v1 revision explicitly defines them.

### 2.2 `schema`

The only v1 value is:

```yaml
schema: ttyinv/v1
```

A future incompatible dialect must use a new identifier such as `ttyinv/v2`.

### 2.3 `invoice`

Required:

```yaml
invoice:
  number: INV-2026-001
  issued: 2026-01-15
  currency: EUR
```

Optional:

```yaml
  title: Consulting services
  kind: standard
  due: 2026-01-29
  locale: en-GB
  terms: Net 14
```

`reference` is not a v1 field. An `invoice.reference` field is rejected as unknown.

`kind` is optional. Its values are `standard` and `gst`.

`standard` renders the invoice total in words. `gst` also renders the received settlement amount in words.


Rules:

- dates use ISO `YYYY-MM-DD`;
- `currency` is one uppercase three-letter code;
- the invoice currency is the payable currency;
- `locale` controls money and decimal formatting only;
- `number`, `terms`, and `title` are text, not numbers;
- `kind` is `standard` or `gst` when supplied.


### 2.4 Parties

`from` and `to` share the party shape:

```yaml
from:
  name: Northstar Studio
  address:
    - 10 Example Street
    - Paris
    - France
  identifiers:
    VAT: FR00000000000
  email: billing@example.com
  website: https://example.com
  logo: ./assets/logo.svg
```

Only `name` is required. `address` is an ordered list. `identifiers` is an ordered label/value mapping displayed as authored. `logo` is a local asset path resolved under the path rules in section 8. `logo_alt` is not a v1 field and is rejected as unknown. Authored logo alternative text is a v2 candidate.

### 2.5 Payment methods

```yaml
payment:
  title: Payment
  pageBreakBefore: false
  methods:
    - title: Bank transfer
      fields:
        Beneficiary: Northstar Studio
        Reference: INV-2026-001
        Instructions: Contact billing@example.com for transfer details
```
`title`, `methods`, and `pageBreakBefore` are optional. `pageBreakBefore` requests a page break before the Payment section. Payment fields are generic label/value pairs. The core schema does not interpret an IBAN, routing number, UPI handle, payment URL, or other payment rail. Payment renders in its own terminal-style frame near the closing edge of the final page.

### 2.6 Settlement records

A settlement records what happened after issuance and does not change the original invoice total:

```yaml
settlements:
  - date: 2026-02-01
    paid:
      amount: "5200.00"
      currency: EUR
    received:
      amount: "574100.25"
      currency: INR
```

Amounts are decimal strings. A settlement is informational and never triggers a live conversion.

### 2.7 Signature

```yaml
signature:
  image: ./assets/signature.svg
  name: Example Person
  label: Authorized signature
```

The image is embedded in self-contained HTML. The signature block follows Payment. Signature images are optional and must never be committed to the public repository when they identify a real person. Signature `alt` is not a v1 field and is rejected as unknown. Authored signature alternative text is a v2 candidate.

### 2.8 Appearance

Portable documents may request supported appearance values:

```yaml
appearance:
  font:
    family: Geist Mono
    regular: ./assets/geist-mono-regular.woff2
    bold: ./assets/geist-mono-bold.woff2
  accent: "#50a6ed"
  paper: "#ffffff"
  ink: "#161618"
  muted: "#68686f"
  rule: "#303038"
  density: comfortable
```

`theme` is not a v1 authored field and is rejected as unknown. Select the render theme with the `--theme` option. `font` is an object with optional `family`, `regular`, and `bold` strings. `rule` is the optional rule color. CLI flags take precedence. A font must resolve to an installed or bundled monospace font. Colors must pass the safe CSS color parser. `density` is `comfortable` or `compact`; it must not alter calculations or page size.
## 3. Sections

A level-two heading defines one named section:

```md
## Contract fees
```

The visible label uses bracket notation and one common left inset:

```text
[ Contract fees ]
```

A heading followed by a GFM table is a table section. A heading followed by prose is a prose section. Section titles are presentation text and do not determine financial behavior by themselves.

An optional `<!-- ttyinv:page-break-before -->` marker immediately before a
level-two heading requests a deterministic page break before that section.
The marker is consumed as layout metadata and is not rendered as invoice text.

An optional `<!-- ttyinv:summary-only -->` marker in the same position marks a
recap table. Its rows remain visible, but the section contributes nothing to
the generated invoice total. This avoids double-counting values that already
appear in earlier sections. The two markers may be adjacent before one heading.

## 4. Tables

### 4.1 Heading row

Every table must include a Markdown heading row and separator row:

```md
| Description | Days | Rate | Amount (EUR) |
| --- | ---: | ---: | ---: |
```

The heading row is part of the visual output. Column alignment markers are honored. Tables remain visually borderless; precision comes from the shared column grid and horizontal rules.

### 4.2 Description details

The literal `<br>` token may separate a primary description from secondary muted detail:

```md
| Systems consulting<br>1 Jan 2026 to 15 Jan 2026 | 8 | 650.00 | auto |
```

No other raw HTML extension is specified by v1.

### 4.3 Financial table detection

A table is financial when it contains an `Amount` column or a currency-qualified amount such as `Amount (EUR)`.

When several amount columns exist, the column whose qualifier matches `invoice.currency` is payable. Other amount columns are informational source-currency values. If no qualifier matches, the final amount column is payable.

### 4.4 Column aliases

The following normalized headings participate in automatic calculation:

Quantity:

```text
Qty
Quantity
Days
Hours
Units
```

Unit price:

```text
Rate
Unit price
Unit_price
Price
```

Amount:

```text
Amount
Amount (CUR)
```

Other columns are rendered as authored and do not affect calculations.

### 4.5 Table layout

The renderer derives a deterministic `<colgroup>` from semantic headings. Description receives flexible space; quantitative columns receive stable right-aligned widths; the final payable amount shares a right edge with table rules and total blocks. Light and dark themes use identical geometry.

## 5. Money and calculations

These rules are planned for the Rust engine. The current Rust `validate` command does not parse quantity, rate, or amount cells. It currently accepts incomplete `auto`, non-numeric amount cells, and explicit mismatches. The Python compatibility engine is the only implementation that enforces these money rules today.

### 5.1 Decimal arithmetic

Planned financial arithmetic uses decimal values, never binary floating-point values.

### 5.2 Automatic amounts

A payable amount cell that is blank or contains `auto` is planned to be calculated when both a recognized quantity and unit-price cell are numeric:

```text
line amount = quantity × unit price
```

If calculation inputs are incomplete, `auto` is planned to be an error.

### 5.3 Explicit amounts

A numeric amount is planned to be explicit. When quantity and unit price are also calculable:

- matching explicit value: accepted;
- differing explicit value: error by default;
- `--trust-explicit`: planned Rust option to use the authored value;
- `--recalculate`: planned Rust option to ignore the authored value and calculate.

The two options are mutually exclusive. The Rust adapter currently rejects both options.

### 5.4 Totals

The planned Rust renderer derives:

- each line amount;
- each financial section subtotal where needed;
- the invoice total due.

Authors may include `Subtotal`, `Total`, or `Grand Total` rows in a table when
round-tripping an existing invoice. Their payable cells must be explicit
numeric amounts; they are rendered as authored metadata rows and excluded from
the generated section and invoice totals, so they are never double-counted.
Generated subtotal and `Total due` blocks remain available for the normal
calculated presentation.

### 5.5 Explicit-only rows

A row with a payable numeric amount and no quantity/rate inputs is planned to be valid. This supports reimbursements, credits, negotiated adjustments, source-currency conversions, and rounding corrections.

### 5.6 No tax engine

`ttyinv/v1` has no jurisdiction-aware or arithmetic tax engine. Tax identifiers may be displayed through party identifiers. Authors who need a tax line may represent it as an explicit financial row, but `ttyinv` does not determine legal applicability, rates, reverse charge, filing language, or compliance.

### 5.7 Formatting direction

Structured dates are locale-independent `YYYY-MM-DD` values everywhere.

Planned `ttyinv/v2` replaces `invoice.locale` with a small set of immutable formatting presets plus direct overrides. This preset system is not implemented in `ttyinv/v1`.

## 6. Output

Rust rendering for `ttyinv/v1` is planned. The compatibility renderer currently implements this output contract during migration.

### 6.1 Formats

Supported output:

```text
HTML
PDF
both
```

HTML is self-contained: CSS, selected fonts, logos, signatures, and local invoice images are embedded. Linked documents remain links, not attachments.

### 6.2 A4

V1 page size is A4 only. Light is the default PDF theme. Dark output must be requested explicitly.

### 6.3 Page frame

Every page has one terminal-style dashed outer frame and four literal `+` corner glyphs. Each glyph box is centered on its frame intersection; the same relational alignment must hold for every supported font.

### 6.4 Borderless tables

Tables do not have outer boxes or vertical cell borders. Header rules, ending rules, subtotal rules, and the total-due rule align to the same generated column grid.

## 7. Pagination

A conforming renderer should:

- repeat `<thead>` on continuation pages;
- avoid splitting rows when practical;
- keep a section label with the beginning of its table;
- keep totals together;
- keep Payment and signature blocks together where practical;
- move a closing block rather than shrink typography;
- preserve the outer frame on every page.

Pagination may differ between browser revisions, fonts, and operating systems; the relational geometry contract must still hold.

## 8. Paths, assets, and links

### 8.1 Resolution root

Relative paths resolve from the directory containing the source Markdown file.

### 8.2 Sandbox

By default, local paths must resolve inside that directory after symlink resolution. Traversal outside the root is an error. `--allow-outside-root` is an explicit trust override.

### 8.3 Remote content

HTTP(S) links remain hyperlinks. The renderer does not fetch remote assets during invoice generation.

### 8.4 PDF links

Web, email, and fragment links should remain clickable. Relative local-document links are best effort in PDF because viewers enforce different policies. `ttyinv lint --require-link-targets` may be used to verify that local targets exist.

## 9. Accessibility

Self-contained HTML should include:

- a declared language;
- a document landmark;
- semantic `<table>`, `<thead>`, `<tbody>`, and `<tfoot>` structures;
- scoped table headings;
- accessible captions or names for invoice tables;
- meaningful alternative text for logos/signatures when supplied;
- sufficient contrast warnings for parseable custom palettes;
- ordinary link semantics.

Decorative frame strokes are hidden from assistive technology.

## 10. Determinism

`--deterministic` removes volatile HTML metadata and normalizes PDF metadata and document IDs. Byte-for-byte reproducibility additionally requires fixed:

- source Markdown and local assets;
- selected font bytes;
- `ttyinv` version;
- Playwright/Chromium revision;
- platform and rasterization environment.
## 11. Diagnostics

Diagnostic records contain twelve fields. Required fields always have values. Optional fields are omitted from JSON when unavailable; they are never emitted as `null`.

| Field | Required | Meaning |
| --- | --- | --- |
| `severity` | yes | Typed severity, serialized as lowercase `error` or `warning` |
| `code` | yes | Diagnostic code from the typed taxonomy |
| `message` | yes | Fixed human-readable explanation without authored document content |
| `path` | no | File path exactly as supplied by the user |
| `field_path` | no | Canonical document path using the canvas path grammar |
| `line` | no | One-based source line |
| `column` | no | One-based source column |
| `hint` | no | Suggested correction |
| `section` | no | Authored section title |
| `section_index` | no | One-based section index |
| `row` | no | One-based table row |
| `column_name` | no | Table heading |

The `path` field belongs to the adapter. The core does not set it. `field_path` uses the same grammar as the canvas contract, such as `settlements[2].date`. `column_name` is omitted when the column has no heading. Human output renders available fields in compiler-style form and omits unavailable optional components.

The complete diagnostic taxonomy is:

| Code | Severity | Meaning |
| --- | --- | --- |
| `FRONTMATTER001` | error | Frontmatter is missing its opening or closing delimiter |
| `YAML001` | error | YAML is malformed |
| `SCHEMA001` | error | Frontmatter contains an explicit null or an unknown field |
| `SCHEMA002` | error | The schema value is not supported |
| `SCHEMA003` | error | A required value is absent or blank |
| `CURRENCY001` | error | The currency code is invalid |
| `DATE001` | error | The date is invalid |
| `DATE002` | error | The due date precedes the issue date |
| `MARKDOWN001` | error | A heading is malformed because an H1 is present or an authored H2 is empty |
| `MARKDOWN002` | warning | The document contains no financial table |
| `MARKDOWN003` | warning | The document contains no H2 section |
| `TABLE001` | error | A table has fewer than two headings |
| `TABLE002` | error | A table has no body rows |
| `TABLE003` | error | A body row width differs from the heading width |
| `TABLE004` | error | A financial section contains a second table |
| `HTML001` | error | The document contains unsupported raw HTML |
| `LIMIT001` | error | The diagnostic limit was reached |
| `INPUT001` | error | The CLI cannot read the input or the input is not valid UTF-8 |
| `INPUT002` | error | The input exceeds `MAX_SOURCE_BYTES` |

Warnings print but do not make the document invalid. Errors make the document invalid.

Human output follows the compiler-style shape:

```text
invoice.md:24:1: error[SCHEMA001]: invalid frontmatter
```

`ttyinv lint --json` emits the compatibility renderer's diagnostic records. The Rust `ttyinv validate --json <file>` command emits Rust diagnostics and includes warning diagnostics. Optional JSON fields are omitted when unavailable.

Process exit codes are:

| Code | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Document invalid because one or more error diagnostics were emitted |
| `2` | Usage error |
| `3` | Input error: unreadable input, non-UTF-8 input, or input above the size bound |
| `4` | Output error: the CLI cannot write the requested output |
| `5` | Reserved for render failure; unused until rendering lands |
| `70` | Internal error |

A document can contain several failure classes at once. Therefore, a failure class belongs in the diagnostic code and never in one exit status. The exit status reports only the process-level result.

## 12. Compatibility policy

`ttyinv/v1` never reached a published release. The append-only guarantee begins at the first tagged release. It does not apply retroactively.

Before the first tagged release, the project may redesign the v1 dialect. After that release, changes within `ttyinv/v1` are append-only and must preserve the financial meaning of every previously valid document.

### 12.1 Implementation direction

One Rust engine owns every invoice rule. The native CLI, WebAssembly, REST, and MCP surfaces are adapters to that engine. The Python and TypeScript engines are temporary references pending deletion.

Allowed after the first tagged release:

- new optional frontmatter fields;
- new unambiguous heading aliases;
- additional diagnostics;
- accessibility improvements;
- layout fixes that preserve documented geometry and semantics;
- stricter rejection of unsafe paths or CSS injection.

Not allowed after the first tagged release:

- changing the meaning of an existing field or column;
- silently changing which amount column is payable;
- changing amount precedence or arithmetic;
- removing a supported field;
- interpreting prose as financial data;
- enabling remote content fetches;
- changing A4 to another default page size.

An incompatible change requires a new schema identifier. A document must never be silently upgraded to a new financial interpretation.
