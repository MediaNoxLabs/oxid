// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import http from "node:http";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, test } from "node:test";

import {
  OFFER_CAPABILITY_LENGTH,
  preparePrivateCapabilityPaths,
  SingleUseOfferHandoff,
} from "./portal-mobile-offer-handoff.mjs";

const servers = new Set();
test("private path preparation rejects a symlinked directory before touching its target", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "oxid-capability-path-"));
  try {
    const realDirectory = path.join(root, "real");
    const linkedDirectory = path.join(root, "linked");
    fs.mkdirSync(realDirectory, { mode: 0o700 });
    const target = path.join(realDirectory, "portal-offer.capability");
    fs.writeFileSync(target, "attacker-visible-placeholder", { mode: 0o600 });
    fs.symlinkSync(realDirectory, linkedDirectory, "dir");
    assert.throws(
      () => preparePrivateCapabilityPaths(
        linkedDirectory,
        "portal-offer.capability",
        ".portal-offer.capability.tmp-1",
      ),
      /real private directory/,
    );
    assert.equal(fs.readFileSync(target, "utf8"), "attacker-visible-placeholder");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

afterEach(async () => {
  await Promise.all([...servers].map((server) => new Promise((resolve) => server.close(resolve))));
  servers.clear();
});

async function fixture() {
  const handoff = new SingleUseOfferHandoff();
  const server = http.createServer((request, response) => {
    if (!handoff.handle(request, response)) {
      response.writeHead(404).end();
    }
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  servers.add(server);
  return {
    handoff,
    origin: `http://127.0.0.1:${server.address().port}`,
  };
}

function arm(handoff, value = "openid-credential-offer://?credential_offer=private") {
  const offer = Buffer.from(value);
  let capability;
  handoff.arm(offer, (bytes) => { capability = Buffer.from(bytes); });
  assert.equal(capability.length, OFFER_CAPABILITY_LENGTH);
  return { capability, offer, value };
}

function authorized(capability) {
  return { headers: { Authorization: `Bearer ${capability.toString("ascii")}` } };
}

test("only the privately provisioned capability consumes an armed offer", async () => {
  const { handoff, origin } = await fixture();
  const { capability, offer, value } = arm(handoff);

  for (const [path, options] of [
    ["/offer", {}],
    ["/offer?capability=public", authorized(capability)],
    ["/offer", { headers: { Authorization: `Bearer ${"f".repeat(64)}` } }],
    ["/offer", { headers: { Authorization: "Bearer short" } }],
  ]) {
    const response = await fetch(`${origin}${path}`, options);
    assert.equal(response.status, 404);
    assert.equal(await response.text(), '{"error":"not_found"}');
    assert.equal(handoff.state, "ready");
  }

  const response = await fetch(`${origin}/offer`, authorized(capability));
  assert.equal(response.status, 200);
  assert.equal(await response.text(), value);
  assert.equal(handoff.state, "consumed");
  assert.ok(offer.every((byte) => byte === 0));
});

test("successful consumption cannot be replayed", async () => {
  const { handoff, origin } = await fixture();
  const { capability } = arm(handoff);
  assert.equal((await fetch(`${origin}/offer`, authorized(capability))).status, 200);
  const replay = await fetch(`${origin}/offer`, authorized(capability));
  assert.equal(replay.status, 404);
  assert.equal(await replay.text(), '{"error":"not_found"}');
});

test("concurrent authorized requests admit exactly one consumer", async () => {
  const { handoff, origin } = await fixture();
  const { capability, value } = arm(handoff);
  const results = await Promise.all(Array.from({ length: 24 }, async () => {
    const response = await fetch(`${origin}/offer`, authorized(capability));
    return { status: response.status, body: await response.text() };
  }));
  assert.equal(results.filter(({ status, body }) => status === 200 && body === value).length, 1);
  assert.equal(results.filter(({ status }) => status === 404).length, 23);
  assert.equal(handoff.state, "consumed");
});

test("cleanup zeroizes unconsumed capability and offer buffers", () => {
  const handoff = new SingleUseOfferHandoff();
  const offer = Buffer.from("private-offer");
  let capabilityReference;
  handoff.arm(offer, (capability) => { capabilityReference = capability; });
  handoff.dispose();
  assert.equal(handoff.state, "empty");
  assert.ok(offer.every((byte) => byte === 0));
  assert.ok(capabilityReference.every((byte) => byte === 0));
});
