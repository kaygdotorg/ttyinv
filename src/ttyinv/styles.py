BASE_CSS = r"""
:root {
  --paper: #ffffff;
  --canvas: #e7e7e9;
  --ink: #141416;
  --muted: #5d5d63;
  --rule: #a3a3aa;
  --accent: #126aa8;
  --font-family: "ttyinv Geist Mono", "Geist Mono", ui-monospace, "SFMono-Regular", Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
  --font-weight-strong: 600;
  --stroke: 0.34mm;
  --page-inset: 16mm;
}
html[data-theme="dark"] {
  --signature-filter: invert(1) grayscale(1);
  --paper: #121214;
  --canvas: #27272a;
  --ink: #f2f2f3;
  --muted: #aaaaaf;
  --rule: #515157;
  --accent: #58a9e8;
}
* { box-sizing: border-box; }
html { background: var(--canvas); }
body {
  margin: 0;
  background: var(--canvas);
  color: var(--ink);
  font-family: var(--font-family);
  font-size: 8.4pt;
  line-height: 1.46;
  font-kerning: normal;
  font-optical-sizing: auto;
  font-variant-numeric: tabular-nums;
  font-variant-ligatures: none;
  text-rendering: geometricPrecision;
  -webkit-font-smoothing: antialiased;
}
a { color: var(--accent); text-underline-offset: 0.18em; }
strong { font-weight: var(--font-weight-strong); color: var(--ink); }
code { font: inherit; padding: 0.08em 0.25em; border: var(--stroke) dashed var(--rule); }
.invoice-sheet {
  position: relative;
  width: 210mm;
  min-height: 297mm;
  margin: 18mm auto;
  padding: 24mm 20mm 22mm;
  background: var(--paper);
  color: var(--ink);
  box-shadow: 0 12mm 28mm rgba(0, 0, 0, 0.18);
  display: flex;
  flex-direction: column;
  box-decoration-break: clone;
  -webkit-box-decoration-break: clone;
}
.page-frame {
  position: absolute;
  inset: var(--page-inset);
  border: var(--stroke) dashed var(--rule);
  pointer-events: none;
  z-index: 1;
}
.frame-corner {
  position: absolute;
  z-index: 2;
  color: var(--rule);
  background: var(--paper);
  line-height: 1;
  padding: 0 0.5mm;
  transform: translate(-50%, -50%);
}
.frame-corner.tl { top: var(--page-inset); left: var(--page-inset); }
.frame-corner.tr { top: var(--page-inset); left: calc(100% - var(--page-inset)); }
.frame-corner.bl { top: calc(100% - var(--page-inset)); left: var(--page-inset); }
.frame-corner.br { top: calc(100% - var(--page-inset)); left: calc(100% - var(--page-inset)); }
.invoice-badge { position: absolute; z-index: 3; top: 14.4mm; left: 50%; transform: translateX(-50%); padding: 0 4mm; white-space: nowrap; color: var(--accent); background: var(--paper); }
.invoice-header { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 18mm; min-height: 29.3mm; padding: 4.5mm 0 9.3mm; border-bottom: var(--stroke) dashed var(--rule); }
.brand-row { display: flex; gap: 1.5mm; align-items: center; align-self: start; }
.brand-logo { width: 4.5mm; height: 4.5mm; object-fit: contain; }
.brand-name { font-size: 13.2pt; line-height: 1; font-weight: var(--font-weight-strong); letter-spacing: -0.03em; }
.invoice-meta { display: grid; grid-template-columns: auto auto; column-gap: 8mm; row-gap: 0.8mm; margin: 0; min-width: 66mm; align-self: start; }
.invoice-meta dt { color: var(--muted); }
.invoice-meta dd { margin: 0; text-align: right; color: var(--ink); }
.invoice-meta dd:first-of-type { font-weight: var(--font-weight-strong); }
.parties { display: grid; grid-template-columns: 1fr 1fr; gap: 22mm; min-height: 53mm; padding: 4.5mm 0 6.5mm; border-bottom: var(--stroke) dashed var(--rule); }
.block-label { color: var(--muted); margin-bottom: 1.5mm; }
.party-name { font-weight: var(--font-weight-strong); }
.party-lines, .identifier-lines { color: var(--muted); }
.identifier-lines { margin-top: 1.5mm; }
.party-contact { margin-top: 1.5mm; }
.preamble { padding: 4mm 0; }
.document-section { position: relative; margin: 5mm 0 0; padding: 5mm 0 0; border-top: var(--stroke) dashed var(--rule); }
.parties + main > .document-section:first-child { margin-top: calc(-1 * var(--stroke)); }
.prose-section { break-inside: avoid; }
.prose-section h3 { font-size: inherit; font-weight: 400; color: var(--muted); }
.prose-section p, .preamble p { margin: 0 0 2mm; }
.prose-section ul, .prose-section ol { margin: 1mm 0 2mm; padding-left: 6mm; }
.prose-section blockquote { margin: 2mm 0; padding-left: 4mm; border-left: var(--stroke) dashed var(--rule); color: var(--muted); }
.section-heading, .payment-label { position: absolute; left: 4mm; top: 0; margin: 0; padding: 0 2mm; color: var(--muted); background: var(--paper); font-weight: 400; font-size: inherit; line-height: 1; white-space: nowrap; transform: translateY(-50%); }
.invoice-table { width: 100%; border: 0; border-spacing: 0; border-collapse: collapse; table-layout: fixed; }
.invoice-table thead { display: table-header-group; }
.invoice-table th { padding: 0 2mm 1.8mm 0; color: var(--muted); font-weight: 400; text-align: left; border: 0; border-bottom: var(--stroke) dashed var(--rule); }
.invoice-table td { border: 0; vertical-align: top; padding: 2.5mm 2mm 1.8mm 0; }
.invoice-table th:not(:first-child), .invoice-table td:not(:first-child) { padding-left: 2mm; }
.invoice-table th:last-child, .invoice-table td:last-child { padding-right: 0; }
.invoice-table tbody tr { break-inside: avoid; page-break-inside: avoid; }
.cell-detail { display: block; margin-top: 0.35mm; color: var(--muted); }
.align-right { text-align: right !important; }
.align-center { text-align: center !important; }
.numeric, .invoice-table th.numeric, .invoice-table td.numeric { text-align: right !important; white-space: nowrap; font-variant-numeric: tabular-nums; }
.table-end-rule { width: 100%; height: 0; margin-top: 0.8mm; border-top: var(--stroke) dashed var(--rule); }
.aligned-summary { display: grid; width: 100%; align-items: baseline; column-gap: 0; font-variant-numeric: tabular-nums; }
.aligned-summary .summary-rule { height: 2.4mm; border-top: var(--stroke) dashed var(--rule); }
.aligned-summary .summary-label { color: var(--muted); padding-right: 4mm; text-align: right; white-space: nowrap; overflow: visible; }
.aligned-summary .summary-amount { color: var(--ink); text-align: right; white-space: nowrap; }
.section-total { margin-top: 2.2mm; padding-bottom: 0.8mm; }
.grand-total { margin-top: 3.2mm; font-weight: var(--font-weight-strong); break-inside: avoid; }
.grand-total .summary-label { color: var(--ink); }
.amount-words { margin-top: 8mm; break-inside: avoid-page; page-break-inside: avoid; }
.amount-words p { margin: 0; max-width: 135mm; color: var(--ink); }
.footer-stack { margin-top: 16mm; padding: 0 0 4mm; }
.payment-frame { position: relative; margin: 0; padding: 7mm 4mm 6mm; border: var(--stroke) dashed var(--rule); break-inside: avoid; }
.payment-method + .payment-method { margin-top: 4mm; padding-top: 4mm; border-top: var(--stroke) dashed var(--rule); }
.payment-method-title { color: var(--muted); margin-bottom: 1mm; }
.payment-fields { display: grid; grid-template-columns: max-content 1fr; gap: 0.5mm 7mm; }
.payment-fields dt { color: var(--muted); }
.payment-fields dd { margin: 0; overflow-wrap: anywhere; }
.signature { margin: 16mm 0 0; break-inside: avoid-page; page-break-inside: avoid; }
.signature img { display: block; filter: var(--signature-filter, none); max-width: 45mm; max-height: 12mm; object-fit: contain; object-position: left bottom; margin-bottom: 1mm; }
.signature-name { font-weight: var(--font-weight-strong); }
.signature-label { color: var(--muted); }
@page { size: A4; margin: 0; }
@media print {
  html, body { background: var(--paper); }
  body { print-color-adjust: exact; -webkit-print-color-adjust: exact; }
  .invoice-sheet { width: 210mm; min-height: 297mm; margin: 0; padding: 24mm 20mm 22mm; box-shadow: none; display: block; }
  .page-frame, .frame-corner, .invoice-badge { position: fixed; }
}
"""

BASE_CSS += r"""
:root {
  --body-font-size: 8.4pt;
  --body-line-height: 1.46;
  --section-gap: 5mm;
  --section-pad: 5mm;
  --row-pad-top: 2.5mm;
  --row-pad-bottom: 1.8mm;
  --party-min-height: 53mm;
  --party-pad-top: 4.5mm;
  --party-pad-bottom: 6.5mm;
}
html[data-density="compact"] {
  --body-font-size: 7.8pt;
  --body-line-height: 1.36;
  --section-gap: 3.8mm;
  --section-pad: 4mm;
  --row-pad-top: 1.7mm;
  --row-pad-bottom: 1.2mm;
  --party-min-height: 46mm;
  --party-pad-top: 3.5mm;
  --party-pad-bottom: 5mm;
}
body { font-size: var(--body-font-size); line-height: var(--body-line-height); }
.parties { min-height: var(--party-min-height); padding-top: var(--party-pad-top); padding-bottom: var(--party-pad-bottom); }
.document-section { margin-top: var(--section-gap); padding-top: var(--section-pad); }
.invoice-table td { padding-top: var(--row-pad-top); padding-bottom: var(--row-pad-bottom); }
.visually-hidden { position: absolute !important; width: 1px !important; height: 1px !important; padding: 0 !important; margin: -1px !important; overflow: hidden !important; clip: rect(0, 0, 0, 0) !important; white-space: nowrap !important; border: 0 !important; }
.section-heading { break-after: avoid-page; page-break-after: avoid; }
.financial-section, .invoice-table, .invoice-table tbody { break-inside: auto; page-break-inside: auto; }
.invoice-table caption { text-align: left; }
.invoice-table thead { display: table-header-group; break-after: avoid-page; page-break-after: avoid; }
.invoice-table tbody tr { break-inside: avoid-page; page-break-inside: avoid; }
.section-tail, .grand-total, .payment-frame, .signature { break-inside: avoid-page; page-break-inside: avoid; }
.section-tail { min-height: 5mm; }
@media print {
  .financial-section { orphans: 2; widows: 2; }
  .invoice-table thead { display: table-header-group; }
}
"""
