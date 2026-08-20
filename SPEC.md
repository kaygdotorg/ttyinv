# ttyinv/v1 specification

Status: **stable v1 dialect**

This document defines the portable Markdown invoice format accepted by `ttyinv`. The format is intentionally narrow: YAML frontmatter carries structured invoice metadata; level-two Markdown headings and GFM tables carry titled sections; prose carries notes and links. The renderer must reject ambiguity rather than infer financial intent creatively.

## 1. Document structure

A document is UTF-8 text with:

1. one YAML frontmatter mapping delimited by `---` at the beginning of the file;
2. zero or more H2 sections;
3. GFM tables or Markdown prose within those sections.

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
invoice: {}
from: {}
to: {}
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
  due: 2026-01-29
  locale: en-GB
  reference: PROJECT-EXAMPLE
  terms: Net 14
```

Rules:

- dates use ISO `YYYY-MM-DD`;
- `currency` is one uppercase three-letter code;
- the invoice currency is the payable currency;
- `locale` affects display formatting, not calculation semantics;
- `number`, `reference`, and `terms` are text, not numbers.

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
  logo_alt: Northstar Studio mark
```

Only `name` is required. `address` is an ordered list. `identifiers` is an ordered label/value mapping displayed as authored. `logo` is a local asset path resolved under the path rules in section 8.

### 2.5 Payment methods

```yaml
payment:
  title: Payment
  methods:
    - title: Bank transfer
      fields:
        Beneficiary: Northstar Studio
        Reference: INV-2026-001
        Instructions: Contact billing@example.com for transfer details
```

Payment fields are generic label/value pairs. The core schema does not interpret an IBAN, routing number, UPI handle, payment URL, or other payment rail. Payment renders in its own terminal-style frame near the closing edge of the final page.

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
  alt: Signature of Example Person
```

The image is embedded in self-contained HTML. The signature block follows Payment. Signature images are optional and must never be committed to the public repository when they identify a real person.

### 2.8 Appearance

Portable documents may request supported appearance values:

```yaml
appearance:
  theme: light
  font: Geist Mono
  accent: "#50a6ed"
  paper: "#ffffff"
  ink: "#161618"
  muted: "#68686f"
  density: comfortable
```

CLI flags take precedence. `font` must resolve to an installed or bundled monospace font. Colors must pass the safe CSS color parser. `density` is `comfortable` or `compact`; it must not alter calculations or page size.

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

### 5.1 Decimal arithmetic

Financial arithmetic uses decimal values, never binary floating-point values.

### 5.2 Automatic amounts

A payable amount cell that is blank or contains `auto` is calculated when both a recognized quantity and unit-price cell are numeric:

```text
line amount = quantity × unit price
```

If calculation inputs are incomplete, `auto` is an error.

### 5.3 Explicit amounts

A numeric amount is explicit. When quantity and unit price are also calculable:

- matching explicit value: accepted;
- differing explicit value: error by default;
- `--trust-explicit`: use the authored value;
- `--recalculate`: ignore the authored value and calculate.

The two flags are mutually exclusive.

### 5.4 Totals

The renderer derives:

- each line amount;
- each financial section subtotal where needed;
- the invoice total due.

Authors should not add `TOTAL` rows. Such rows are diagnosed because totals are generated consistently by the renderer. A single-section invoice may omit a redundant subtotal and show only `Total due`.

### 5.5 Explicit-only rows

A row with a payable numeric amount and no quantity/rate inputs is valid. This supports reimbursements, credits, negotiated adjustments, source-currency conversions, and rounding corrections.

### 5.6 No tax engine

`ttyinv/v1` has no jurisdiction-aware or arithmetic tax engine. Tax identifiers may be displayed through party identifiers. Authors who need a tax line may represent it as an explicit financial row, but `ttyinv` does not determine legal applicability, rates, reverse charge, filing language, or compliance.

## 6. Output

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

Every page has a terminal-style outer frame. The four junctions are constructed from intersecting strokes rather than font `+` glyphs. Junction centers and rule axes must align independently of the selected font.

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

Diagnostics have:

```text
severity
code
message
path
line
column
hint
```

Human output follows the compiler-style shape:

```text
invoice.md:24:1: error[MONEY004]: explicit amount differs from quantity × rate
```

`ttyinv lint --json` emits the same data as JSON.

## 12. Compatibility policy

Within `ttyinv/v1`, changes are append-only and must preserve the financial meaning of every previously valid document.

Allowed within v1:

- new optional frontmatter fields;
- new unambiguous heading aliases;
- additional diagnostics;
- accessibility improvements;
- layout fixes that preserve documented geometry and semantics;
- stricter rejection of unsafe paths or CSS injection.

Not allowed within v1:

- changing the meaning of an existing field or column;
- silently changing which amount column is payable;
- changing amount precedence or arithmetic;
- removing a supported field;
- interpreting prose as financial data;
- enabling remote content fetches;
- changing A4 to another default page size.

An incompatible change requires a new schema identifier. A document must never be silently upgraded to a new financial interpretation.
