// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = relative => readFileSync(path.join(root, relative), "utf8");

test("support journal store stays owner-private, bounded, and uncomposed", () => {
  const source = read("crates/adapters/diagnostics-encrypted/src/store.rs");
  const manifest = read("crates/adapters/diagnostics-encrypted/Cargo.toml");
  const composition = read("crates/composition/Cargo.toml");

  assert.match(manifest, /oxid-adapter-store-atomic\.workspace = true/u);
  assert.match(source, /MAX_SUPPORT_JOURNAL_DURABLE_RECORDS/u);
  assert.match(source, /MAX_SUPPORT_JOURNAL_DURABLE_BYTES/u);
  assert.match(source, /MAX_SUPPORT_JOURNAL_RETENTION_DAYS/u);
  assert.match(source, /read_owner_private_bounded/u);
  assert.match(source, /write_owner_private/u);
  assert.match(source, /protected_latest_head/u);
  assert.doesNotMatch(source, /serde|serde_json|tokio|reqwest|tracing/u);
  assert.doesNotMatch(composition, /oxid-adapter-diagnostics-encrypted/u);
});

test("support journal recovery covers corruption, interruption, expiry, and eviction", () => {
  const source = read("crates/adapters/diagnostics-encrypted/src/store.rs");
  for (const name of [
    "append_and_recover_preserve_order_and_protected_head",
    "append_rejects_a_rolled_back_manifest",
    "manifest_disagreement_never_quarantines_valid_ciphertext",
    "unauthenticated_expiry_edit_never_deletes_segments",
    "interrupted_head_commit_rolls_back_to_the_protected_manifest",
    "backwards_clock_does_not_regress_archive_retention",
    "permissive_orphan_is_degraded_hygiene_not_archive_failure",
    "corruption_quarantines_only_the_failed_segment",
    "wrong_key_never_quarantines_the_first_segment",
    "missing_segment_truncates_the_unrecoverable_suffix",
    "expiry_clears_segments_on_first_activation",
    "expiry_removes_quarantined_ciphertext",
    "interrupted_orphan_is_removed_but_unknown_files_are_untouched",
    "symlinked_segment_is_rejected_without_touching_target",
    "record_limit_evicts_oldest_segments_deterministically",
  ]) assert.match(source, new RegExp(`fn ${name}\\(`, "u"));
});
