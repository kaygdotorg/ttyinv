# Privacy

The public repository contains only Rust source, fabricated v2 examples, and reviewed
font assets. It does not require invoice content, credentials, or personal data.

Do not commit real invoices, customer names, addresses, bank details, signatures, image
files, private keys, environment files, or access tokens. Use `example.com` identities in
fixtures.

The Rust CLI reads the source path supplied by its caller. The core does not fetch remote
images, links, fonts, or scripts. The WASM adapter processes source in memory and returns
structured diagnostics and edit responses.
