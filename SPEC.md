# ttyinv/v1 specification

## 1. Scope

`ttyinv/v1` is a strict Markdown invoice dialect rendered as A4 HTML and PDF.

The source document is self-contained with respect to invoice data. It may reference local binary assets and supporting documents by path.

The renderer performs arithmetic only. It does not determine taxes, exchange rates, legal wording, or jurisdictional compliance.

## 2. Document structure

A source file consists of:

1. YAML frontmatter delimited by `---`.
2. A Markdown body containing titled tables and optional prose sections.
3. Optional relative paths to logos, signatures, fonts, images, and supporting documents.

The frontmatter root must include:

```yaml
schema: ttyinv/v1
invoice: ...
from: ...
to: ...
```

Unknown schema fields are rejected.

## 3. Frontmatter

### 3.1 `invoice`

```yaml
invoice:
  number: INV-2026-004       # required
  title: Invoice             # optional; default Invoice
  issued: 20 Aug 2026        # required; rendered verbatim
  due: 3 Sep 2026            # optional; rendered verbatim
  terms: Net 14              # optional
  currency: EUR              # required; three-letter code
  locale: en-GB              # optional; default en-GB
```

The invoice has one payable currency. `locale` controls monetary and numeric formatting.

### 3.2 Parties

`from` and `to` share the same schema:

```yaml
from:
  name: Example Studio       # required
  logo: ./assets/logo.svg    # optional
  address:                   # optional
    - 10 Example Street
    - Paris
    - France
  identifiers:               # optional ordered mapping
    VAT: FR00000000000
  email: billing@example.com # optional
  website: https://example.com # optional
```

Address entries and identifier values are rendered in their authored order.

### 3.3 Payment methods

```yaml
payment:
  title: Payment
  methods:
    - title: SEPA
      fields:
        Beneficiary: Example Studio
        IBAN: DE00 0000 0000 0000 0000 00
        BIC: EXAMPLE0XXX
        Bank: Example Bank
```

`fields` is an arbitrary ordered mapping of labels to string values. Payment methods render inside a dedicated ASCII-style frame near the end of the invoice.

### 3.4 Signature

```yaml
signature:
  image: ./assets/signature.svg
  name: Avery Example
  label: Authorised signature
```

All signature fields are optional, but the section is useful only when at least one is present. Local images are embedded into HTML.

### 3.5 Settlements

Settlement information is typed frontmatter rather than a financial Markdown table, so it cannot be counted as new invoice revenue:

```yaml
settlements:
  - date: 28 Sep 2026
    paid:
      amount: 5018.10
      currency: EUR
    received:
      amount: 478000.00
      currency: INR
```

`received` is optional. Settlements do not affect the invoice grand total.

### 3.6 Appearance

```yaml
appearance:
  accent: "#2685d2"
  font:
    family: Invoice Mono
    regular: ./assets/Mono-Regular.woff2
    bold: ./assets/Mono-Bold.woff2
```

`accent` accepts an injection-safe CSS color value such as a hex color, named color, `rgb(...)`, `hsl(...)`, or `oklch(...)`. The CLI `--accent COLOR` overrides the frontmatter value. WOFF, WOFF2, TTF, and OTF font files can be embedded. When no font is configured, Geist Mono regular and semibold are the canonical defaults. Release distributions embed those webfonts; an unvendored source checkout may fall back to a locally installed Geist Mono or the system monospace stack with a warning. `ttyinv --list-fonts` lists installed families verified as monospace, and `--font FAMILY` embeds one of those families for a single render.

## 4. Markdown body

### 4.1 Financial sections

A level-two heading immediately followed by one GFM table defines a financial section:

```md
## Contract fees

| Description | Days | Rate | Amount (EUR) |
| :--- | ---: | ---: | ---: |
| Systems engineering<br>August 2026 | 20 | 250.00 | auto |
```

Rules:

- The heading is rendered as a bracketed label inset from the left edge of the section rule, using the same label geometry as the Payment block.
- The Markdown table header is rendered as written.
- A financial section contains exactly one table and no additional body content.
- Put notes or supporting prose in a separate level-two section.
- Do not author a `TOTAL` row; `ttyinv` generates section and grand totals.

### 4.2 Prose sections

A level-two heading not followed immediately by a table defines a prose section:

```md
## Notes

Payment is due within fourteen days. See the [previous invoice](./INV-003.pdf).
```

The CommonMark parser supports paragraphs, lists, links, images, emphasis, code, blockquotes, and other normal Markdown constructs. Raw HTML is disabled. Escaped `<br>` in table cells is converted into a line break.

## 5. Table column semantics

Column labels are preserved visually. A small set of normalized labels participates in calculations.

Quantity aliases:

```text
Qty, Quantity, Days, Hours, Units
```

Rate aliases:

```text
Rate, Unit price, Price
```

Description aliases:

```text
Description, Item, Service
```

An amount column is any header beginning with `Amount`, or the exact header `Total`.

A financial section must resolve to exactly one payable amount column:

- `Amount` is payable when it is the only amount column.
- `Amount (<invoice currency>)` is payable.
- Other currency-marked amount columns are source information.

Example for an EUR invoice:

```text
Amount (KZT) -> source amount
Amount (EUR) -> payable amount
```

## 6. Amount calculation

For each row:

- blank or `auto` amount plus quantity and rate: calculate quantity × rate;
- explicit amount plus quantity and rate: verify the values match within 0.005 currency units;
- explicit amount without both quantity and rate: accept the amount;
- no explicit amount and no calculable quantity/rate: error.

Arithmetic uses Python `Decimal`, not binary floating point.

### 6.1 Policy flags

Default behavior rejects an explicit mismatch.

`--trust-explicit` retains the authored amount when it differs from quantity × rate.

`--recalculate` replaces an authored amount whenever quantity and rate are available.

The flags are mutually exclusive.

## 7. Multi-currency behavior

The invoice has one payable currency. Source-currency table values and settlement values may use other currencies.

Converted payable values must be authored explicitly. `ttyinv` does not perform exchange-rate lookup.

## 8. Assets and links

Paths are resolved relative to the source Markdown file.

- Logos, signatures, Markdown images, and configured fonts are embedded as data URLs.
- HTTP, HTTPS, `mailto:`, `data:`, and `#anchor` references are preserved.
- Relative document links are rebased relative to HTML output.
- For PDF, local document links become absolute `file:` URLs and generate a best-effort warning.
- Missing local targets generate a warning.
- Local linked documents are not embedded as PDF attachments in v1.

## 9. Output

- Page size: A4 only.
- Default theme: light.
- Optional theme: dark.
- Both themes share geometry, typography metrics, label placement, and pagination.
- HTML includes inline CSS and embedded configured assets.
- PDF is printed from the same HTML through Chromium using Playwright.

## 10. Non-goals

v1 does not include:

- a web app or editor;
- storage, authentication, or sending;
- invoice-number allocation;
- a tax engine;
- exchange-rate fetching;
- jurisdictional compliance claims;
- embedded supporting-document attachments;
- page sizes other than A4.
