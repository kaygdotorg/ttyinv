# Security

The Rust core validates source size, frontmatter keys, dates, currencies, table shapes,
section order, directive placement, and edit revisions.

The engine does not fetch images, links, fonts, scripts, or stylesheets. Adapters resolve
local assets outside the core. WASM requests enforce decoded size limits. Edit requests
require the exact base revision and return `CONFLICT001` on stale input.

Report security issues privately to the maintainers. Do not include real invoices,
credentials, private keys, or personal data in reports or fixtures.
