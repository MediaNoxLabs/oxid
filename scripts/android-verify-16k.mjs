#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { readFileSync } from "node:fs";
import path from "node:path";
import { inflateRawSync } from "node:zlib";

const PAGE_SIZE = 16 * 1024;
const LOAD = 1;

function fail(member, message) {
  throw new Error(`${member}: ${message}`);
}

function u64(view, offset, member, field) {
  const value = view.getBigUint64(offset, true);
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) fail(member, `${field} exceeds supported size`);
  return Number(value);
}

function elfLoadSegments(bytes, member) {
  if (bytes.length < 52 || bytes[0] !== 0x7f || bytes[1] !== 0x45 || bytes[2] !== 0x4c || bytes[3] !== 0x46) {
    fail(member, "is not an ELF shared library");
  }
  if (bytes[5] !== 1) fail(member, "ELF is not little-endian");

  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const elfClass = bytes[4];
  let programOffset;
  let programEntrySize;
  let programCount;
  let offsetAt;
  let virtualAddressAt;
  let alignAt;

  if (elfClass === 1) {
    programOffset = view.getUint32(28, true);
    programEntrySize = view.getUint16(42, true);
    programCount = view.getUint16(44, true);
    offsetAt = 4;
    virtualAddressAt = 8;
    alignAt = 28;
  } else if (elfClass === 2) {
    if (bytes.length < 64) fail(member, "ELF header is truncated");
    programOffset = u64(view, 32, member, "ELF program-header offset");
    programEntrySize = view.getUint16(54, true);
    programCount = view.getUint16(56, true);
    offsetAt = 8;
    virtualAddressAt = 16;
    alignAt = 48;
  } else {
    fail(member, "has an unsupported ELF class");
  }

  if (programEntrySize === 0 || programOffset + programEntrySize * programCount > bytes.length) {
    fail(member, "ELF program headers are truncated");
  }

  const loads = [];
  for (let index = 0; index < programCount; index += 1) {
    const start = programOffset + index * programEntrySize;
    if (view.getUint32(start, true) !== LOAD) continue;
    const read = elfClass === 1
      ? (at, field) => view.getUint32(start + at, true)
      : (at, field) => u64(view, start + at, member, field);
    loads.push({
      index,
      offset: read(offsetAt, "LOAD offset"),
      virtualAddress: read(virtualAddressAt, "LOAD virtual address"),
      align: read(alignAt, "LOAD alignment"),
    });
  }
  if (loads.length === 0) fail(member, "ELF has no LOAD segments");
  return loads;
}

function verifyElf(bytes, member) {
  for (const load of elfLoadSegments(bytes, member)) {
    if (load.align < PAGE_SIZE || load.align % PAGE_SIZE !== 0) {
      fail(member, `ELF LOAD segment ${load.index} alignment ${load.align} is not 16 KiB compatible`);
    }
    if (load.offset % PAGE_SIZE !== load.virtualAddress % PAGE_SIZE) {
      fail(member, `ELF LOAD segment ${load.index} offset and virtual address are not 16 KiB congruent`);
    }
  }
}

function zipMembers(archive) {
  const view = new DataView(archive.buffer, archive.byteOffset, archive.byteLength);
  const minimumEnd = Math.max(0, archive.length - 0xffff - 22);
  let end = -1;
  for (let offset = archive.length - 22; offset >= minimumEnd; offset -= 1) {
    if (
      view.getUint32(offset, true) === 0x06054b50
      && offset + 22 + view.getUint16(offset + 20, true) === archive.length
    ) {
      end = offset;
      break;
    }
  }
  if (end < 0) throw new Error("APK: ZIP end-of-central-directory record is missing");
  const entries = view.getUint16(end + 10, true);
  const centralOffset = view.getUint32(end + 16, true);
  const members = [];
  let offset = centralOffset;
  for (let index = 0; index < entries; index += 1) {
    if (offset + 46 > archive.length || view.getUint32(offset, true) !== 0x02014b50) {
      throw new Error("APK: ZIP central-directory record is truncated");
    }
    const method = view.getUint16(offset + 10, true);
    const compressedSize = view.getUint32(offset + 20, true);
    const uncompressedSize = view.getUint32(offset + 24, true);
    const nameLength = view.getUint16(offset + 28, true);
    const extraLength = view.getUint16(offset + 30, true);
    const commentLength = view.getUint16(offset + 32, true);
    const localOffset = view.getUint32(offset + 42, true);
    const name = new TextDecoder().decode(archive.subarray(offset + 46, offset + 46 + nameLength));
    if (localOffset + 30 > archive.length || view.getUint32(localOffset, true) !== 0x04034b50) {
      fail(name, "ZIP local header is missing");
    }
    const localNameLength = view.getUint16(localOffset + 26, true);
    const localExtraLength = view.getUint16(localOffset + 28, true);
    const dataOffset = localOffset + 30 + localNameLength + localExtraLength;
    if (dataOffset + compressedSize > archive.length) fail(name, "ZIP member data is truncated");
    members.push({ name, method, compressedSize, uncompressedSize, dataOffset });
    offset += 46 + nameLength + extraLength + commentLength;
  }
  return members;
}

export function verifyApk(archive, archiveName = "APK") {
  const nativeMembers = zipMembers(archive).filter(({ name }) => name.endsWith(".so"));
  if (nativeMembers.length === 0) {
    throw new Error(`${archiveName}: APK contains no native shared libraries`);
  }
  for (const member of nativeMembers) {
    const stored = archive.subarray(member.dataOffset, member.dataOffset + member.compressedSize);
    let elfBytes;
    if (member.method === 0) {
      if (member.compressedSize !== member.uncompressedSize) {
        fail(member.name, "stored ZIP member has inconsistent sizes");
      }
      if (member.dataOffset % PAGE_SIZE !== 0) {
        fail(member.name, `ZIP data offset ${member.dataOffset} is not aligned to 16 KiB`);
      }
      elfBytes = stored;
    } else if (member.method === 8) {
      try {
        elfBytes = inflateRawSync(stored);
      } catch {
        fail(member.name, "compressed ZIP member cannot be decompressed");
      }
      if (elfBytes.length !== member.uncompressedSize) {
        fail(member.name, "compressed ZIP member has inconsistent size");
      }
    } else {
      fail(member.name, `uses unsupported ZIP compression method ${member.method}`);
    }
    verifyElf(elfBytes, member.name);
  }
  return nativeMembers.length;
}

function main() {
  const apk = process.argv[2];
  if (!apk || process.argv.length !== 3) {
    console.error("Usage: android-verify-16k.mjs <apk>");
    process.exitCode = 2;
    return;
  }
  try {
    const count = verifyApk(readFileSync(apk), path.basename(apk));
    console.log(`android-verify-16k: PASS apk=${apk} native-libraries=${count}`);
  } catch (error) {
    console.error(`android-verify-16k: FAIL ${error.message}`);
    process.exitCode = 1;
  }
}

if (import.meta.url === `file://${process.argv[1]}`) main();
