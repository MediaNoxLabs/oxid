# Headless protocol fixtures

`protocol-wire.expected.ndjson` was captured from the pre-refactor
`oxid-headless` binary built at
`integration@ba2b767558756746663f15bcbd94b5bee27221de`. It was re-verified by
feeding `protocol-wire.ndjson` to that isolated baseline binary and comparing
the output byte-for-byte with `cmp`.

- input SHA-256: `b59092ebd9a4007258b3ceb5c86c134820e71a9326bfc51174723950e55899ab`
- expected/baseline-output SHA-256:
  `9212f0c0e3633fa5be0a00b37a42db2dfb50703f7c6c4cf467b2f9bade8b4ea1`

The golden corpus deliberately excludes diagnostics buffer capacity, event
sequence values, and collection ordering. Those fields are exercised by a
separate structural protocol test so internal storage changes do not create a
misleading byte-fixture failure.
