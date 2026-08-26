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

## dev-loops agent compatibility shadows

The tracked `.pi/agents/*.agent.md` compatibility shadows are derived from
`dev-loops@0.9.0` agent manifests, with repository-specific tool allowlists,
entrypoints, and read-only context rules. The source package is distributed
under the MIT License:

MIT License

Copyright (c) 2026 mfittko

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

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
