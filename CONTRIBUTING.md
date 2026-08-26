# Contributing

Use Rust 1.85 or newer. Use the workspace commands from the repository root.

```console
make check
make rust-release
make wasm
```

Keep the Rust core as the one source of parsing, validation, serialization, and edit
operations. Keep CLI and WASM adapters thin. Update `SPEC.md` when grammar changes.
Update `schema/ttyinv-v2.schema.json` when configuration changes.

Add a fabricated v2 example for each new grammar case. Add a focused Rust test for each
new observable contract. Do not add compatibility parsers or aliases.

Do not commit private invoices, generated PDFs, credentials, or unreviewed font files.
Do not commit build output. Do not deploy from a development checkout.

The project uses AGPL-3.0-only. The font files in `assets/fonts` retain their OFL terms.
