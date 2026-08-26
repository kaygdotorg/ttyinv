# ttyinv engineering rules

## Simplified Technical English

Use short active sentences. Keep each instruction below 20 words. Keep technical names,
paths, commands, and symbols exact.

## One source

Keep grammar, limits, registries, labels, and diagnostics in `ttyinv-core`. Keep CLI and
WASM adapters thin. Do not add compatibility parsers, aliases, or deprecated paths.

## Rust workflow

Run `make check` after source changes. Run `make rust-release` before release work. Run
`make wasm` after WASM export changes. Keep tests focused on observable contracts.

## Security

Do not commit private invoices, credentials, generated output, or unreviewed fonts. Use
fabricated identities and `example.com` addresses in fixtures. Do not deploy from this
repository.
