# Releasing

Run the Rust workspace gates before a release.

```console
make check
make rust-release
make wasm
```

Review the v2 examples and render-compat corpus. Verify that generated output is absent
from the source tree. Verify that public fixtures use fabricated identities and
`example.com` addresses.

Publish the Rust CLI and WASM artifacts with their checksums. Keep the generated schema
at `schema/ttyinv-v2.schema.json`. Do not publish private invoice data.
