// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFile, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";
import { deflateRawSync } from "node:zlib";

import { verifyApk } from "./android-verify-16k.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const script = path.join(root, "scripts", "android-verify-16k.mjs");
const PAGE_SIZE = 16 * 1024;
const member = "lib/arm64-v8a/liboxid.so";
const ELF_HEADER_SIZE = 64;
const PROGRAM_HEADER_SIZE = 56;
const PROGRAM_HEADER_COUNT = 2;
const SECTION_HEADER_SIZE = 64;
const DYNAMIC_SYMBOL_ENTRY_SIZE = 24;

function elf({
  alignment = PAGE_SIZE,
  fileOffset = 0,
  fileSize = 0,
  memorySize = fileSize,
  virtualAddress = 0,
  programEntrySize = 56,
  dynamicSymbolCount = 1,
  hash = "gnu",
  dynamicHash = true,
} = {}) {
  const dynamicSymbolsSize = dynamicSymbolCount * DYNAMIC_SYMBOL_ENTRY_SIZE;
  const hashSize = 32;
  const dynamicOffset = ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE * PROGRAM_HEADER_COUNT;
  const dynamicSize = hash === "missing" ? 16 : 32;
  const dynamicSymbolsOffset = dynamicOffset + dynamicSize;
  const hashOffset = dynamicSymbolsOffset + dynamicSymbolsSize;
  const sectionOffset = hashOffset + (hash === "missing" ? 0 : hashSize);
  const sectionCount = hash === "missing" ? 2 : 3;
  const bytes = Buffer.alloc(sectionOffset + sectionCount * SECTION_HEADER_SIZE);
  bytes.set([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1]);
  bytes.writeUInt16LE(3, 16);
  bytes.writeUInt16LE(183, 18);
  bytes.writeUInt32LE(1, 20);
  bytes.writeBigUInt64LE(64n, 32);
  bytes.writeBigUInt64LE(BigInt(sectionOffset), 40);
  bytes.writeUInt16LE(64, 52);
  bytes.writeUInt16LE(programEntrySize, 54);
  bytes.writeUInt16LE(PROGRAM_HEADER_COUNT, 56);
  bytes.writeUInt16LE(SECTION_HEADER_SIZE, 58);
  bytes.writeUInt16LE(sectionCount, 60);
  bytes.writeUInt32LE(1, 64);
  bytes.writeBigUInt64LE(BigInt(fileOffset), 72);
  bytes.writeBigUInt64LE(BigInt(virtualAddress), 80);
  bytes.writeBigUInt64LE(BigInt(virtualAddress), 88);
  bytes.writeBigUInt64LE(BigInt(fileSize), 96);
  bytes.writeBigUInt64LE(BigInt(memorySize), 104);
  bytes.writeBigUInt64LE(BigInt(alignment), 112);

  const dynamicProgramHeader = ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE;
  bytes.writeUInt32LE(2, dynamicProgramHeader);
  bytes.writeBigUInt64LE(BigInt(dynamicOffset), dynamicProgramHeader + 8);
  bytes.writeBigUInt64LE(BigInt(dynamicOffset), dynamicProgramHeader + 16);
  bytes.writeBigUInt64LE(BigInt(dynamicOffset), dynamicProgramHeader + 24);
  bytes.writeBigUInt64LE(BigInt(dynamicSize), dynamicProgramHeader + 32);
  bytes.writeBigUInt64LE(BigInt(dynamicSize), dynamicProgramHeader + 40);
  bytes.writeBigUInt64LE(8n, dynamicProgramHeader + 48);
  if (hash !== "missing" && dynamicHash) {
    bytes.writeBigUInt64LE(hash === "sysv" ? 4n : 0x6ffffef5n, dynamicOffset);
    bytes.writeBigUInt64LE(BigInt(hashOffset), dynamicOffset + 8);
  }

  const dynamicSymbols = sectionOffset + SECTION_HEADER_SIZE;
  bytes.writeUInt32LE(11, dynamicSymbols + 4);
  bytes.writeBigUInt64LE(BigInt(dynamicSymbolsOffset), dynamicSymbols + 24);
  bytes.writeBigUInt64LE(BigInt(dynamicSymbolsSize), dynamicSymbols + 32);
  bytes.writeBigUInt64LE(BigInt(DYNAMIC_SYMBOL_ENTRY_SIZE), dynamicSymbols + 56);
  if (hash !== "missing") {
    const dynamicHash = dynamicSymbols + SECTION_HEADER_SIZE;
    bytes.writeUInt32LE(hash === "sysv" ? 5 : 0x6ffffff6, dynamicHash + 4);
    bytes.writeBigUInt64LE(BigInt(hashOffset), dynamicHash + 24);
    bytes.writeBigUInt64LE(BigInt(hashSize), dynamicHash + 32);
  }
  return bytes;
}

function apk({
  archiveAligned = true,
  centralCopies = 1,
  centralExtraLength = 0,
  centralFlags = 0,
  centralStartDisk = 0,
  compressedPadding = 0,
  compressedSizeDelta = 0,
  declaredUncompressedSize,
  endCentralDirectoryDisk = 0,
  endDisk = 0,
  entriesOnDisk = centralCopies,
  localFlags = 0,
  method = 0,
  memberName = member,
  ...elfOptions
} = {}) {
  const payload = Buffer.concat([elf(elfOptions), Buffer.alloc(compressedPadding)]);
  const stored = method === 8 ? deflateRawSync(payload) : payload;
  const name = Buffer.from(memberName);
  const extraLength = archiveAligned ? PAGE_SIZE - 30 - name.length : 0;
  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50, 0);
  local.writeUInt16LE(localFlags, 6);
  local.writeUInt16LE(method, 8);
  local.writeUInt16LE(name.length, 26);
  local.writeUInt16LE(extraLength, 28);
  const dataOffset = local.length + name.length + extraLength;
  const centralOffset = dataOffset + stored.length;
  const central = Buffer.alloc(46);
  central.writeUInt32LE(0x02014b50, 0);
  central.writeUInt16LE(centralFlags, 8);
  central.writeUInt16LE(method, 10);
  central.writeUInt16LE(name.length, 28);
  central.writeUInt16LE(centralExtraLength, 30);
  central.writeUInt16LE(centralStartDisk, 34);
  central.writeUInt32LE(stored.length + compressedSizeDelta, 20);
  central.writeUInt32LE(declaredUncompressedSize ?? payload.length, 24);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(endDisk, 4);
  end.writeUInt16LE(endCentralDirectoryDisk, 6);
  end.writeUInt16LE(entriesOnDisk, 8);
  end.writeUInt16LE(centralCopies, 10);
  end.writeUInt32LE((central.length + name.length) * centralCopies, 12);
  end.writeUInt32LE(centralOffset, 16);
  const centralRecords = Array.from({ length: centralCopies }, () => [central, name]).flat();
  return Buffer.concat([local, name, Buffer.alloc(extraLength), stored, ...centralRecords, end]);
}

test("accepts a hermetic APK with a 16 KiB ZIP placement and ELF LOAD alignment", () => {
  assert.equal(verifyApk(apk()), 1);
});

test("accepts either Android loader hash-table format", () => {
  assert.equal(verifyApk(apk({ hash: "sysv" })), 1);
});

test("rejects a native library without a dynamic hash table", () => {
  assert.throws(
    () => verifyApk(apk({ hash: "missing" })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ELF dynamic hash table is missing`),
  );
});

test("rejects an orphan hash section that the loader cannot discover", () => {
  assert.throws(
    () => verifyApk(apk({ dynamicHash: false })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ELF DT_HASH/DT_GNU_HASH entry is missing`),
  );
});

test("rejects a native library without inspectable section headers", () => {
  const bytes = apk();
  bytes.writeBigUInt64LE(0n, PAGE_SIZE + 40);
  bytes.writeUInt16LE(0, PAGE_SIZE + 60);
  assert.throws(
    () => verifyApk(bytes),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ELF section headers are missing or truncated`),
  );
});

test("bounds the dynamic symbol surface before Android installation", () => {
  assert.throws(
    () => verifyApk(apk({ dynamicSymbolCount: 65_537 })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ELF dynamic symbol count 65537 exceeds 65536`),
  );
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

test("rejects a non-power-of-two ELF LOAD alignment", () => {
  assert.throws(
    () => verifyApk(apk({ alignment: 49152 })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ELF LOAD segment 0 alignment 49152`),
  );
});

test("checks ELF LOAD congruence against the declared alignment", () => {
  assert.throws(
    () => verifyApk(apk({ alignment: 65536, fileOffset: 64 })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ELF LOAD segment 0 offset and virtual address`),
  );
});

test("rejects a LOAD segment whose file extent exceeds the decoded ELF", () => {
  assert.throws(
    () => verifyApk(apk({ fileOffset: PAGE_SIZE, fileSize: PAGE_SIZE })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ELF LOAD segment 0 extends beyond`),
  );
});

test("rejects a LOAD segment whose file size exceeds its memory size", () => {
  assert.throws(
    () => verifyApk(apk({ fileSize: 1, memorySize: 0 })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ELF LOAD segment 0 file size exceeds its memory size`),
  );
});

test("rejects an ELF whose declared program-header entries are undersized", () => {
  assert.throws(
    () => verifyApk(apk({ programEntrySize: 1 })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ELF program headers are truncated`),
  );
});

test("fails closed when an APK contains no native shared libraries", () => {
  assert.throws(
    () => verifyApk(apk({ memberName: "assets/not-a-library.bin" }), "empty.apk"),
    /empty\.apk: APK contains no native shared libraries/,
  );
});

test("rejects truncated central-directory variable fields", () => {
  assert.throws(
    () => verifyApk(apk({ centralExtraLength: 65535 })),
    /APK: ZIP central-directory variable fields are truncated/,
  );
});

test("rejects a native member encrypted according to its central-directory header", () => {
  assert.throws(
    () => verifyApk(apk({ centralFlags: 1 })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: encrypted ZIP members are unsupported`),
  );
});

test("rejects a native member encrypted according to its local header", () => {
  assert.throws(
    () => verifyApk(apk({ localFlags: 1 })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: encrypted ZIP members are unsupported`),
  );
});

test("rejects multi-disk end-of-central-directory metadata", () => {
  assert.throws(
    () => verifyApk(apk({ endDisk: 1 })),
    /APK: ZIP end-of-central-directory uses unsupported multi-disk metadata/,
  );
  assert.throws(
    () => verifyApk(apk({ endCentralDirectoryDisk: 1 })),
    /APK: ZIP end-of-central-directory uses unsupported multi-disk metadata/,
  );
});

test("rejects inconsistent per-disk and total ZIP entry counts", () => {
  assert.throws(
    () => verifyApk(apk({ entriesOnDisk: 0 })),
    /APK: ZIP end-of-central-directory entry counts are inconsistent/,
  );
});

test("rejects a central-directory member that starts on another disk", () => {
  assert.throws(
    () => verifyApk(apk({ centralStartDisk: 1 })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ZIP central-directory entry starts on unsupported disk`),
  );
});

test("rejects a native member whose claimed data overlaps the central directory", () => {
  assert.throws(
    () => verifyApk(apk({ compressedSizeDelta: 1 })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: ZIP member data overlaps the central directory`),
  );
});

test("bounds compressed native members by their declared uncompressed size", () => {
  assert.throws(
    () => verifyApk(apk({ method: 8, compressedPadding: 64 * 1024, declaredUncompressedSize: 120 })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: compressed ZIP member exceeds its declared size`),
  );
});

test("rejects an attacker-controlled compressed size above the inspection limit", () => {
  assert.throws(
    () => verifyApk(apk({ method: 8, declaredUncompressedSize: 0xffffffff })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: compressed ZIP member exceeds the 512 MiB inspection safety limit`),
  );
});

test("bounds aggregate decompression across repeated native members", () => {
  assert.throws(
    () => verifyApk(apk({ method: 8, centralCopies: 2, declaredUncompressedSize: 300 * 1024 * 1024 })),
    new RegExp(`${member.replace(/[/.]/g, "\\$&")}: compressed native members exceed the aggregate 512 MiB inspection safety limit`),
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

test("the command executes when its filesystem path requires URL encoding", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "oxid android 16k-"));
  try {
    const encodedScript = path.join(directory, "verifier #1.mjs");
    const fixture = path.join(directory, "compliant.apk");
    await copyFile(script, encodedScript);
    await writeFile(fixture, apk());
    const result = spawnSync(process.execPath, [encodedScript, fixture], { cwd: root, encoding: "utf8" });
    assert.equal(result.status, 0);
    assert.match(result.stdout, /PASS.*native-libraries=1/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("the command executes through a preserved main-module symlink", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "oxid-android-16k-"));
  try {
    const linkedScript = path.join(directory, "verifier.mjs");
    const fixture = path.join(directory, "compliant.apk");
    await symlink(script, linkedScript);
    await writeFile(fixture, apk());
    const result = spawnSync(process.execPath, ["--preserve-symlinks-main", linkedScript, fixture], {
      cwd: root,
      encoding: "utf8",
    });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /PASS.*native-libraries=1/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("the verifier can be imported by an eval process without a main-script argument", () => {
  const result = spawnSync(
    process.execPath,
    ["--input-type=module", "--eval", `import ${JSON.stringify(pathToFileURL(script).href)}`],
    { cwd: root, encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, "");
});

test("an eval import stays inert when argv contains the verifier path", () => {
  const result = spawnSync(
    process.execPath,
    ["--input-type=module", "--eval", `import ${JSON.stringify(pathToFileURL(script).href)}`, script, "candidate.apk"],
    { cwd: root, encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, "");
});

test("the verifier can be imported by a stdin module whose argv entry is not a path", () => {
  const result = spawnSync(process.execPath, ["--input-type=module", "-"], {
    cwd: root,
    encoding: "utf8",
    input: `import ${JSON.stringify(pathToFileURL(script).href)};`,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, "");
});
