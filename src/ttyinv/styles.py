BASE_CSS = r"""
:root {
  --paper: #ffffff;
  --canvas: #e7e7e9;
  --ink: #171719;
  --muted: #68686d;
  --rule: #aaaab0;
  --accent: #126aa8;
  --font-family: "ttyinv Geist Mono", "Geist Mono", ui-monospace, "SFMono-Regular", Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
  --font-weight-strong: 600;
  --stroke: 0.34mm;
  --page-inset: 16mm;
}
html[data-theme="dark"] {
  --signature-filter: invert(1) grayscale(1);
  --paper: #161618;
  --canvas: #2a2a2c;
  --ink: #ececef;
  --muted: #9c9ca2;
  --rule: #434349;
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
  padding: 19mm 20mm 16mm;
  background: var(--paper);
  color: var(--ink);
  box-shadow: 0 12mm 28mm rgba(0, 0, 0, 0.18);
  display: flex;
  flex-direction: column;
}
/* The page frame is made from four explicit dashed edges. It must never be
   inferred from a table border or a font glyph. */
.page-frame {
  position: absolute;
  inset: var(--page-inset);
  pointer-events: none;
  z-index: 1;
}
.frame-edge,
.frame-junction { position: absolute; display: block; }
.frame-edge.top,
.frame-edge.bottom {
  left: 1.7mm;
  right: 1.7mm;
  height: 0;
  border-top: var(--stroke) dashed var(--rule);
}
.frame-edge.top { top: 0; }
.frame-edge.bottom { bottom: 0; }
.frame-edge.left,
.frame-edge.right {
  top: 1.7mm;
  bottom: 1.7mm;
  width: 0;
  border-left: var(--stroke) dashed var(--rule);
}
.frame-edge.left { left: 0; }
.frame-edge.right { right: 0; }
.frame-junction {
  width: 3.4mm;
  height: 3.4mm;
  transform: translate(-50%, -50%);
}
.frame-junction::before,
.frame-junction::after {
  content: "";
  position: absolute;
  background: var(--rule);
}
.frame-junction::before {
  left: 0;
  right: 0;
  top: calc(50% - var(--stroke) / 2);
  height: var(--stroke);
}
.frame-junction::after {
  top: 0;
  bottom: 0;
  left: calc(50% - var(--stroke) / 2);
  width: var(--stroke);
}
.frame-junction.tl { top: 0; left: 0; }
.frame-junction.tr { top: 0; left: 100%; }
.frame-junction.bl { top: 100%; left: 0; }
.frame-junction.br { top: 100%; left: 100%; }
.invoice-badge {
  position: absolute;
  z-index: 3;
  top: 14.4mm;
  left: 50%;
  transform: translateX(-50%);
  padding: 0 4mm;
  white-space: nowrap;
  color: var(--accent);
  background: var(--paper);
}
.invoice-header {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 18mm;
  min-height: 29.3mm;
  padding: 4.5mm 0 9.3mm;
  border-bottom: var(--stroke) dashed var(--rule);
}
.brand-row { display: flex; gap: 1.5mm; align-items: center; align-self: start; }
.brand-logo { width: 4.5mm; height: 4.5mm; object-fit: contain; }
.brand-name {
  font-size: 13.2pt;
  line-height: 1;
  font-weight: var(--font-weight-strong);
  letter-spacing: -0.03em;
}
.invoice-meta {
  display: grid;
  grid-template-columns: auto auto;
  column-gap: 8mm;
  row-gap: 0.8mm;
  margin: 0;
  min-width: 66mm;
  align-self: start;
}
.invoice-meta dt { color: var(--muted); }
.invoice-meta dd { margin: 0; text-align: right; color: var(--ink); }
.invoice-meta dd:first-of-type { font-weight: var(--font-weight-strong); }
.parties {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 22mm;
  min-height: 53mm;
  padding: 4.5mm 0 6.5mm;
  border-bottom: var(--stroke) dashed var(--rule);
}
.block-label { color: var(--muted); margin-bottom: 1.5mm; }
.party-name { font-weight: var(--font-weight-strong); }
.party-lines, .identifier-lines { color: var(--muted); }
.identifier-lines { margin-top: 1.5mm; }
.party-contact { margin-top: 1.5mm; }
.preamble { padding: 4mm 0; }
.prose-section,
.financial-section {
  position: relative;
  margin: 5mm 0 0;
  padding: 5mm 0 0;
  border-top: var(--stroke) dashed var(--rule);
}
.parties + main > .prose-section:first-child,
.parties + main > .financial-section:first-child {
  margin-top: calc(-1 * var(--stroke));
}
.prose-section { break-inside: avoid; }
.prose-section h3 { font-size: inherit; font-weight: 400; color: var(--muted); }
.prose-section p, .preamble p { margin: 0 0 2mm; }
.prose-section ul, .prose-section ol { margin: 1mm 0 2mm; padding-left: 6mm; }
.prose-section blockquote { margin: 2mm 0; padding-left: 4mm; border-left: var(--stroke) dashed var(--rule); color: var(--muted); }
.section-heading,
.payment-label {
  position: absolute;
  left: 4mm;
  top: -0.86em;
  margin: 0;
  padding: 0 2mm;
  color: var(--muted);
  background: var(--paper);
  font-weight: 400;
  font-size: inherit;
  line-height: inherit;
  white-space: nowrap;
}
/* Financial tables deliberately have no outer box. Precision comes from a
   shared column grid, full-width rules and right-aligned tabular numerals. */
.invoice-table {
  width: 100%;
  border: 0;
  border-spacing: 0;
  border-collapse: collapse;
  table-layout: fixed;
}
.invoice-table thead { display: table-header-group; }
.invoice-table th {
  padding: 0 2mm 1.8mm 0;
  color: var(--muted);
  font-weight: 400;
  text-align: left;
  border: 0;
  border-bottom: var(--stroke) dashed var(--rule);
}
.invoice-table td {
  border: 0;
  vertical-align: top;
  padding: 2.5mm 2mm 1.8mm 0;
}
.invoice-table th:not(:first-child),
.invoice-table td:not(:first-child) { padding-left: 2mm; }
.invoice-table th:last-child,
.invoice-table td:last-child { padding-right: 0; }
.invoice-table tbody tr { break-inside: avoid; page-break-inside: avoid; }
.cell-detail { display: block; margin-top: 0.35mm; color: var(--muted); }
.align-right { text-align: right !important; }
.align-center { text-align: center !important; }
.numeric { text-align: right; white-space: nowrap; font-variant-numeric: tabular-nums; }
.table-end-rule {
  width: 100%;
  height: 0;
  margin-top: 0.8mm;
  border-top: var(--stroke) dashed var(--rule);
}
.aligned-summary {
  display: grid;
  width: 100%;
  align-items: baseline;
  column-gap: 0;
  font-variant-numeric: tabular-nums;
}
.aligned-summary .summary-rule {
  height: 2.4mm;
  border-top: var(--stroke) dashed var(--rule);
}
.aligned-summary .summary-label {
  color: var(--muted);
  padding-right: 4mm;
  text-align: right;
  white-space: nowrap;
  overflow: visible;
}
.aligned-summary .summary-amount {
  color: var(--ink);
  text-align: right;
  white-space: nowrap;
}
.section-total { margin-top: 2.2mm; padding-bottom: 0.8mm; }
.grand-total {
  margin-top: 3.2mm;
  font-weight: var(--font-weight-strong);
  break-inside: avoid;
}
.grand-total .summary-label { color: var(--ink); }
.settlement-block {
  position: relative;
  margin: 5mm 0 0;
  padding-top: 5mm;
  border-top: var(--stroke) dashed var(--rule);
  break-inside: avoid;
}
.settlement-grid {
  display: grid;
  grid-template-columns: 28mm 1fr 1fr;
  gap: 2mm 5mm;
  font-variant-numeric: tabular-nums;
}
.settlement-grid .head { color: var(--muted); }
.settlement-grid .money { text-align: right; }
.footer-stack { margin-top: auto; padding: 6mm 0 4mm; }
.payment-frame {
  position: relative;
  margin: 0;
  padding: 5mm 4mm 4mm;
  border: var(--stroke) dashed var(--rule);
  break-inside: avoid;
}
.payment-method + .payment-method { margin-top: 4mm; padding-top: 4mm; border-top: var(--stroke) dashed var(--rule); }
.payment-method-title { color: var(--muted); margin-bottom: 1mm; }
.payment-fields { display: grid; grid-template-columns: max-content 1fr; gap: 0.5mm 7mm; }
.payment-fields dt { color: var(--muted); }
.payment-fields dd { margin: 0; overflow-wrap: anywhere; }
.signature { margin: 4mm 0 0; break-inside: avoid; }
.signature img { display: block; filter: var(--signature-filter, none); max-width: 45mm; max-height: 12mm; object-fit: contain; object-position: left bottom; margin-bottom: 1mm; }
.signature-name { font-weight: var(--font-weight-strong); }
.signature-label { color: var(--muted); }
@page { size: A4; margin: 0; }
@media print {
  html, body { background: var(--paper); }
  body { print-color-adjust: exact; -webkit-print-color-adjust: exact; }
  .invoice-sheet { width: 210mm; min-height: 297mm; margin: 0; box-shadow: none; }
  .page-frame, .invoice-badge { position: fixed; }
}
"""
