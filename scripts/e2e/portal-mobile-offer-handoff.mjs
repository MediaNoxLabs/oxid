// SPDX-License-Identifier: Apache-2.0

import { randomFillSync, timingSafeEqual } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

export const OFFER_CAPABILITY_LENGTH = 64;

export function preparePrivateCapabilityPaths(directory, targetName, candidateName) {
  if (!path.isAbsolute(directory)
      || !/^[.a-z-]+$/.test(targetName)
      || !/^[.a-z0-9-]+$/.test(candidateName)) {
    throw new Error("capability paths are invalid");
  }
  const directoryMetadata = fs.lstatSync(directory);
  if (!directoryMetadata.isDirectory() || directoryMetadata.isSymbolicLink()) {
    throw new Error("capability directory must be a real private directory");
  }
  fs.chmodSync(directory, 0o700);
  const inspectAndRemove = (candidate) => {
    let metadata;
    try {
      metadata = fs.lstatSync(candidate);
    } catch (error) {
      if (error?.code === "ENOENT") return;
      throw error;
    }
    if (!metadata.isFile() || metadata.isSymbolicLink()) {
      throw new Error("capability target must be a regular non-symlink file");
    }
    fs.rmSync(candidate);
  };
  const target = path.join(directory, targetName);
  const candidate = path.join(directory, candidateName);
  inspectAndRemove(target);
  inspectAndRemove(candidate);
  return { target, candidate };
}
const HEX = Buffer.from("0123456789abcdef", "ascii");

function newCapability() {
  const random = randomFillSync(Buffer.alloc(32));
  const capability = Buffer.alloc(OFFER_CAPABILITY_LENGTH);
  for (let index = 0; index < random.length; index += 1) {
    capability[index * 2] = HEX[random[index] >> 4];
    capability[index * 2 + 1] = HEX[random[index] & 0x0f];
  }
  random.fill(0);
  return capability;
}

function candidateCapability(request) {
  const authorizationHeaders = [];
  for (let index = 0; index < request.rawHeaders.length; index += 2) {
    if (request.rawHeaders[index].toLowerCase() === "authorization") {
      authorizationHeaders.push(request.rawHeaders[index + 1]);
    }
  }
  if (authorizationHeaders.length !== 1) return null;
  const match = /^Bearer ([0-9a-f]{64})$/.exec(authorizationHeaders[0]);
  return match ? Buffer.from(match[1], "ascii") : null;
}

function deny(response) {
  const bytes = Buffer.from('{"error":"not_found"}');
  response.writeHead(404, {
    "Cache-Control": "no-store",
    "Content-Length": bytes.length,
    "Content-Type": "application/json",
  });
  response.end(bytes);
}

/**
 * One armed offer, one authenticated consumer, one response. `arm` transfers
 * ownership of the offer buffer and exposes the capability only to the
 * synchronous private provisioner. The state transition happens before any
 * response byte is scheduled, so concurrent authorized requests cannot race.
 */
export class SingleUseOfferHandoff {
  #state = "empty";
  #capability = null;
  #offer = null;

  get state() {
    return this.#state;
  }

  arm(offer, provisionCapability) {
    if (!Buffer.isBuffer(offer) || offer.length === 0 || typeof provisionCapability !== "function") {
      throw new TypeError("offer handoff requires an owned non-empty Buffer and private provisioner");
    }
    if (this.#state === "ready" || this.#state === "consuming") {
      throw new Error("offer handoff is already armed");
    }
    this.dispose();
    this.#offer = offer;
    this.#capability = newCapability();
    this.#state = "ready";
    try {
      provisionCapability(this.#capability);
    } catch (error) {
      this.dispose();
      throw error;
    }
  }

  handle(request, response) {
    if (request.method !== "GET") return false;
    if (request.url !== "/offer") {
      if (request.url.startsWith("/offer?")) {
        deny(response);
        return true;
      }
      return false;
    }
    const candidate = candidateCapability(request);
    const authorized = this.#state === "ready"
      && candidate !== null
      && this.#capability !== null
      && candidate.length === this.#capability.length
      && timingSafeEqual(candidate, this.#capability);
    candidate?.fill(0);
    if (!authorized) {
      deny(response);
      return true;
    }

    this.#state = "consuming";
    this.#capability.fill(0);
    this.#capability = null;
    const offer = this.#offer;
    this.#offer = null;
    let cleared = false;
    const clear = () => {
      if (cleared) return;
      cleared = true;
      offer.fill(0);
      this.#state = "consumed";
    };
    response.once("close", clear);
    response.writeHead(200, {
      "Cache-Control": "no-store",
      "Content-Length": offer.length,
      "Content-Type": "text/plain; charset=utf-8",
      Pragma: "no-cache",
    });
    response.end(offer, clear);
    return true;
  }

  dispose() {
    this.#capability?.fill(0);
    this.#offer?.fill(0);
    this.#capability = null;
    this.#offer = null;
    this.#state = "empty";
  }
}
