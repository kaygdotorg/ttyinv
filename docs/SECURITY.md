# Security model

`ttyinv` renders local Markdown into HTML and PDF. Invoice files are treated as untrusted input even when they come from the local filesystem.

## Local by design

- The renderer does not fetch remote images, stylesheets, fonts, or scripts.
- HTTP(S) links remain links; they are not downloaded during rendering.
- Local assets are embedded in self-contained HTML output.
- Relative paths are resolved from the Markdown file's directory, not the current shell directory.

## Path sandbox

By default, local assets and file links must resolve inside the invoice directory. This includes symlinks: a symlink that resolves outside the root is rejected.

```console
ttyinv invoice.md --allow-outside-root
```

The escape hatch is deliberately explicit. Use it only for invoices and assets you trust.

## Markdown and HTML

`ttyinv/v1` is a strict dialect, not an arbitrary HTML templating system. YAML and Markdown are parsed as data. Raw HTML is not a supported extension point; the only inline HTML convention in tables is the literal `<br>` separator used for secondary description lines.

## Links in PDF

Web, email, and internal fragment links are retained. Relative local-document links are best effort because PDF viewers apply different security policies. `ttyinv lint --require-link-targets` verifies that local link targets exist before output is generated.

## Secrets and personal data

The repository must contain only fabricated examples. CI runs both the project privacy checker and Gitleaks. Never add real invoices, customer names, postal addresses, tax identifiers, bank coordinates, signatures, private reference images, access tokens, keys, or production environment files.

## Hosted agent access

The optional hosted API and MCP server use personal Bearer tokens created only after GitHub authentication. Tokens are random, shown once, stored as SHA-256 digests, scoped to invoice creation and validation, expiring, revocable, and subject to the hosted service's published rate limits. Token creation is capped, and metadata for tokens expired or revoked beyond the hosted service's retention window is pruned on the account's next token creation. Requests with a browser `Origin` must match the configured application origin; wildcard CORS is not enabled. Request bodies are bounded and validated with strict schemas before ttyinv renders them. See the [hosted service documentation](https://app.ttyinv.com/docs) for current published limits and retention settings.

The hosted editor does not send its local draft to these endpoints. An agent API or MCP call is an explicit opt-in transfer: the server receives that call's invoice fields or Markdown long enough to validate and return the result. It does not persist or log invoice bodies. Reverse proxies may still retain ordinary connection metadata, so operators should keep body logging disabled and protect the token database and backups.

## Reporting a vulnerability

Open a private security advisory in the GitHub repository. Do not include a real invoice or credential in a public issue; use the smallest fabricated reproduction possible.
