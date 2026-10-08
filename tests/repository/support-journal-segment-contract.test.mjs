// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = relative => readFileSync(path.join(root, relative), "utf8");

test("support journal segments are closed, bounded, authenticated, and uncomposed", () => {
  const source = read("crates/adapters/diagnostics-encrypted/src/lib.rs");
  const manifest = read("crates/adapters/diagnostics-encrypted/Cargo.toml");
  const composition = read("crates/composition/Cargo.toml");

  assert.match(source, /events: &\[SupportJournalEvent\]/u);
  assert.match(source, /MAX_SUPPORT_JOURNAL_FLUSH_BATCH/u);
  assert.match(source, /XChaCha20Poly1305/u);
  assert.match(source, /Hmac::<Sha256>/u);
  assert.match(source, /previous_head/u);
  assert.match(source, /Zeroizing<\[u8; 32\]>/u);
  assert.doesNotMatch(source, /\b(?:serde|serde_json|String|PathBuf|std::fs|OpenOptions)\b/u);
  assert.doesNotMatch(manifest, /serde|serde_json|tokio|reqwest|tracing/u);
  assert.doesNotMatch(composition, /oxid-adapter-diagnostics-encrypted/u);
});

test("support journal segment regression suite covers closed failure modes", () => {
  const source = read("crates/adapters/diagnostics-encrypted/src/lib.rs");
  for (const name of [
    "fixed_width_round_trip_preserves_closed_fields",
    "every_closed_enum_value_round_trips",
    "wrong_key_tamper_truncation_and_version_drift_fail_closed",
    "chain_head_rejects_reorder_duplication_and_internal_gaps",
    "batch_shape_is_strictly_bounded_and_ordered",
    "public_seal_uses_fresh_nonces_and_covers_the_maximum_batch",
    "sealed_payload_is_not_the_fixed_plaintext_record",
  ]) assert.match(source, new RegExp(`fn ${name}\\(`, "u"));
});
