// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const adrPath = path.join(root, "docs/adr/0119-bound-a-secret-safe-support-journal.md");
const read = (relative) => readFile(path.join(root, relative), "utf8");

test("support journal decision stays opt-in, non-authoritative, and closed-code", async () => {
  const adr = await readFile(adrPath, "utf8");
  assert.match(adr, /\*\*off by default\*\*/u);
  assert.match(adr, /at most 24 hours/u);
  assert.match(adr, /cannot authorize or decide readiness, retry, consent/u);
  assert.match(adr, /There is no string, byte buffer, map, arbitrary target/u);
  assert.match(adr, /no sanitised-string escape hatch/u);
  assert.match(adr, /There is no upload URL/u);
  assert.match(adr, /there is no fallback to plaintext/u);
});

test("support journal resource and storage envelope is concrete", async () => {
  const adr = await readFile(adrPath, "utf8");
  for (const expected of [
    /Pending channel \| 128 records/u,
    /Archive retention age \| 7 days/u,
    /Durable records \| 4,096 records/u,
    /Durable encoded bytes \| 2 MiB/u,
    /20 records\/second and 200 records\/minute/u,
    /32 records or 2 seconds/u,
    /Export bundle \| 1 MiB and 4,096 records/u,
  ]) assert.match(adr, expected);
  assert.match(adr, /excluded\s+from every backup, recovery, profile transfer, crash report, and telemetry path/u);
  assert.match(adr, /authenticated encryption with unique nonces/u);
  assert.match(adr, /This is not non-repudiation/u);
});

test("support export remains review-first, encrypted, and OS-share-only", async () => {
  const adr = await readFile(adrPath, "utf8");
  assert.match(adr, /Before encryption, the review surface shows/u);
  assert.match(adr, /support-recipient public key shipped in a signed build manifest/u);
  assert.match(adr, /fresh high-entropy one-time secret/u);
  assert.match(adr, /there is no fallback to plaintext/u);
  assert.match(adr, /Delivery uses the OS share\/export surface only/u);
  assert.match(adr, /There is no upload URL, analytics SDK, background sync, live stream/u);
});

test("ADR catalogs and amended process-local boundary remain linked", async () => {
  const [index, site, processLocal] = await Promise.all([
    read("docs/adr/README.md"),
    read("docs/site/src/adr-catalog.md"),
    read("docs/adr/0080-bound-secret-safe-runtime-diagnostics.md"),
  ]);
  for (const catalog of [index, site]) assert.match(catalog, /0119-bound-a-secret-safe-support-journal\.md/u);
  assert.match(processLocal, /Amended by: ADR-0095 and ADR-0119/u);
});
