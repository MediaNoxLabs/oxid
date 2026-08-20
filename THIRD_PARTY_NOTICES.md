# Third-party notices

Oxid includes selected inline icon paths from Lucide, retained through the
reviewed `midnight-ledger` wallet shell. The icons used here are `wallet`,
`fingerprint`, `badge-check`, `activity`, and `settings-2`.

## Lucide

ISC License

Copyright (c) 2026 Lucide Icons and Contributors

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY
AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
PERFORMANCE OF THIS SOFTWARE.

## Midnight Wallet SDK conformance data

Oxid's development-only Midnight account adapter retains three public address
payloads and expected Bech32m encodings from
`packages/address-format/test/addresses.json` at Midnight Wallet SDK commit
`25d0c3857fc0e20435e06a9225bd8709ecce1115`. The seed contained in the upstream
test case is not retained. Midnight Wallet SDK is distributed under the Apache
License 2.0, the same license used by this repository.

## Mermaid (documentation site only)

`docs/site/mermaid.min.js` is the bundled Mermaid diagram renderer
(MIT License, https://github.com/mermaid-js/mermaid), installed verbatim by
`mdbook-mermaid install` and served only on the documentation site. It is
not part of any wallet binary.
