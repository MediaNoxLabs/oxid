// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { deflateRawSync } from "node:zlib";

import { verifyApk } from "./android-verify-16k.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const script = path.join(root, "scripts", "android-verify-16k.mjs");
const PAGE_SIZE = 16 * 1024;
const member = "lib/arm64-v8a/liboxid.so";

function elf({ alignment = PAGE_SIZE, virtualAddress = 0 } = {}) {
  const bytes = Buffer.alloc(64 + 56);
  bytes.set([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1]);
  bytes.writeUInt16LE(3, 16);
  bytes.writeUInt16LE(183, 18);
  bytes.writeUInt32LE(1, 20);
  bytes.writeBigUInt64LE(64n, 32);
  bytes.writeUInt16LE(64, 52);
  bytes.writeUInt16LE(56, 54);
  bytes.writeUInt16LE(1, 56);
  bytes.writeUInt32LE(1, 64);
  bytes.writeBigUInt64LE(0n, 72);
  bytes.writeBigUInt64LE(BigInt(virtualAddress), 80);
  bytes.writeBigUInt64LE(BigInt(virtualAddress), 88);
  bytes.writeBigUInt64LE(0n, 96);
  bytes.writeBigUInt64LE(0n, 104);
  bytes.writeBigUInt64LE(BigInt(alignment), 112);
  return bytes;
}

function apk({ archiveAligned = true, method = 0, memberName = member, ...elfOptions } = {}) {
  const payload = elf(elfOptions);
  const stored = method === 8 ? deflateRawSync(payload) : payload;
  const name = Buffer.from(memberName);
  const extraLength = archiveAligned ? PAGE_SIZE - 30 - name.length : 0;
  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50, 0);
  local.writeUInt16LE(method, 8);
  local.writeUInt16LE(name.length, 26);
  local.writeUInt16LE(extraLength, 28);
  const dataOffset = local.length + name.length + extraLength;
  const centralOffset = dataOffset + stored.length;
  const central = Buffer.alloc(46);
  central.writeUInt32LE(0x02014b50, 0);
  central.writeUInt16LE(method, 10);
  central.writeUInt16LE(name.length, 28);
  central.writeUInt32LE(stored.length, 20);
  central.writeUInt32LE(payload.length, 24);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(1, 8);
  end.writeUInt16LE(1, 10);
  end.writeUInt32LE(central.length + name.length, 12);
  end.writeUInt32LE(centralOffset, 16);
  return Buffer.concat([local, name, Buffer.alloc(extraLength), stored, central, name, end]);
}

test("accepts a hermetic APK with a 16 KiB ZIP placement and ELF LOAD alignment", () => {
  assert.equal(verifyApk(apk()), 1);
});

test("names the exact archive member whose ZIP placement is not 16 KiB aligned", () => {
  assert.throws(() => verifyApk(apk({ archiveAligned: false })), new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ZIP data offset .*16 KiB`));
});

test("accepts a compressed native member after verifying its ELF alignment", () => {
  assert.equal(verifyApk(apk({ method: 8 })), 1);
});

test("names a compressed member whose decompressed ELF alignment is insufficient", () => {
  assert.throws(
    () => verifyApk(apk({ method: 8, alignment: 4096 })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ELF LOAD segment 0 alignment 4096`),
  );
});

test("names the exact archive member whose ELF LOAD alignment is insufficient", () => {
  assert.throws(() => verifyApk(apk({ alignment: 4096 })), new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ELF LOAD segment 0 alignment 4096`));
});

test("fails closed when an APK contains no native shared libraries", () => {
  assert.throws(
    () => verifyApk(apk({ memberName: "assets/not-a-library.bin" }), "empty.apk"),
    /empty\.apk: APK contains no native shared libraries/,
  );
});

test("the documented command reports the exact offending archive member", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "oxid-android-16k-"));
  try {
    const fixture = path.join(directory, "non-compliant.apk");
    await writeFile(fixture, apk({ alignment: 4096 }));
    const result = spawnSync(process.execPath, [script, fixture], { cwd: root, encoding: "utf8" });
    assert.equal(result.status, 1);
    assert.match(result.stderr, new RegExp(`FAIL ${member.replace(/[/.]/g, "\\$&")}: ELF LOAD`));
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
