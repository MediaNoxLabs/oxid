#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { readFileSync } from "node:fs";
import path from "node:path";
import { inflateRawSync } from "node:zlib";

const PAGE_SIZE = 16 * 1024;
const LOAD = 1;
const DYNAMIC = 2;
const DT_NULL = 0n;
const DT_HASH = 4n;
const DT_GNU_HASH = 0x6ffffef5n;
const SHT_HASH = 5;
const SHT_DYNSYM = 11;
const SHT_GNU_HASH = 0x6ffffff6;
const SHT_NOBITS = 8;
const MAX_NATIVE_BYTES = 512 * 1024 * 1024;
const MAX_DYNAMIC_SYMBOLS = 65_536;

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
  let fileSizeAt;
  let memorySizeAt;
  let alignAt;
  let minimumProgramEntrySize;

  if (elfClass === 1) {
    programOffset = view.getUint32(28, true);
    programEntrySize = view.getUint16(42, true);
    programCount = view.getUint16(44, true);
    offsetAt = 4;
    virtualAddressAt = 8;
    fileSizeAt = 16;
    memorySizeAt = 20;
    alignAt = 28;
    minimumProgramEntrySize = 32;
  } else if (elfClass === 2) {
    if (bytes.length < 64) fail(member, "ELF header is truncated");
    programOffset = u64(view, 32, member, "ELF program-header offset");
    programEntrySize = view.getUint16(54, true);
    programCount = view.getUint16(56, true);
    offsetAt = 8;
    virtualAddressAt = 16;
    fileSizeAt = 32;
    memorySizeAt = 40;
    alignAt = 48;
    minimumProgramEntrySize = 56;
  } else {
    fail(member, "has an unsupported ELF class");
  }

  if (
    programEntrySize < minimumProgramEntrySize
    || programOffset + programEntrySize * programCount > bytes.length
  ) {
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
      fileSize: read(fileSizeAt, "LOAD file size"),
      memorySize: read(memorySizeAt, "LOAD memory size"),
      align: read(alignAt, "LOAD alignment"),
    });
  }
  if (loads.length === 0) fail(member, "ELF has no LOAD segments");
  return loads;
}

function elfDynamicShape(bytes, member) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const elfClass = bytes[4];
  let sectionOffset;
  let sectionEntrySize;
  let sectionCount;
  let minimumSectionEntrySize;
  let sectionFileOffsetAt;
  let sectionSizeAt;
  let sectionEntrySizeAt;
  let dynamicSymbolEntrySize;

  if (elfClass === 1) {
    sectionOffset = view.getUint32(32, true);
    sectionEntrySize = view.getUint16(46, true);
    sectionCount = view.getUint16(48, true);
    minimumSectionEntrySize = 40;
    sectionFileOffsetAt = 16;
    sectionSizeAt = 20;
    sectionEntrySizeAt = 36;
    dynamicSymbolEntrySize = 16;
  } else if (elfClass === 2) {
    sectionOffset = u64(view, 40, member, "ELF section-header offset");
    sectionEntrySize = view.getUint16(58, true);
    sectionCount = view.getUint16(60, true);
    minimumSectionEntrySize = 64;
    sectionFileOffsetAt = 24;
    sectionSizeAt = 32;
    sectionEntrySizeAt = 56;
    dynamicSymbolEntrySize = 24;
  } else {
    fail(member, "has an unsupported ELF class");
  }

  if (
    sectionCount === 0
    || sectionEntrySize < minimumSectionEntrySize
    || sectionOffset + sectionEntrySize * sectionCount > bytes.length
  ) {
    fail(member, "ELF section headers are missing or truncated");
  }

  let dynamicSymbols = 0;
  const hashKinds = new Set();
  for (let index = 0; index < sectionCount; index += 1) {
    const start = sectionOffset + index * sectionEntrySize;
    const type = view.getUint32(start + 4, true);
    const read = elfClass === 1
      ? (at) => view.getUint32(start + at, true)
      : (at, field) => u64(view, start + at, member, field);
    const offset = read(sectionFileOffsetAt, "section offset");
    const size = read(sectionSizeAt, "section size");
    const entrySize = read(sectionEntrySizeAt, "section entry size");
    if (type !== SHT_NOBITS && (offset > bytes.length || size > bytes.length - offset)) {
      fail(member, `ELF section ${index} extends beyond the shared library`);
    }
    if (type === SHT_DYNSYM) {
      if (entrySize < dynamicSymbolEntrySize || size === 0 || size % entrySize !== 0) {
        fail(member, "ELF dynamic symbol table is empty or malformed");
      }
      dynamicSymbols += size / entrySize;
    }
    if (type === SHT_HASH || type === SHT_GNU_HASH) {
      if (size === 0) fail(member, "ELF dynamic hash table is empty");
      hashKinds.add(type === SHT_GNU_HASH ? "gnu" : "sysv");
    }
  }
  if (dynamicSymbols === 0) fail(member, "ELF dynamic symbol table is missing");
  if (dynamicSymbols > MAX_DYNAMIC_SYMBOLS) {
    fail(member, `ELF dynamic symbol count ${dynamicSymbols} exceeds ${MAX_DYNAMIC_SYMBOLS}`);
  }
  if (hashKinds.size === 0) fail(member, "ELF dynamic hash table is missing");

  const dynamicHashKinds = new Set();
  const programOffset = elfClass === 1
    ? view.getUint32(28, true)
    : u64(view, 32, member, "ELF program-header offset");
  const programEntrySize = elfClass === 1 ? view.getUint16(42, true) : view.getUint16(54, true);
  const programCount = elfClass === 1 ? view.getUint16(44, true) : view.getUint16(56, true);
  const dynamicEntrySize = elfClass === 1 ? 8 : 16;
  let foundDynamicSegment = false;
  for (let index = 0; index < programCount; index += 1) {
    const start = programOffset + index * programEntrySize;
    if (view.getUint32(start, true) !== DYNAMIC) continue;
    foundDynamicSegment = true;
    const offset = elfClass === 1
      ? view.getUint32(start + 4, true)
      : u64(view, start + 8, member, "DYNAMIC offset");
    const size = elfClass === 1
      ? view.getUint32(start + 16, true)
      : u64(view, start + 32, member, "DYNAMIC file size");
    if (size === 0 || size % dynamicEntrySize !== 0 || offset > bytes.length || size > bytes.length - offset) {
      fail(member, "ELF dynamic segment is empty or malformed");
    }
    for (let entry = offset; entry < offset + size; entry += dynamicEntrySize) {
      const tag = elfClass === 1
        ? BigInt(view.getUint32(entry, true))
        : view.getBigUint64(entry, true);
      if (tag === DT_NULL) break;
      if (tag !== DT_HASH && tag !== DT_GNU_HASH) continue;
      const address = elfClass === 1
        ? BigInt(view.getUint32(entry + 4, true))
        : view.getBigUint64(entry + 8, true);
      if (address === 0n) fail(member, "ELF dynamic hash address is null");
      dynamicHashKinds.add(tag === DT_GNU_HASH ? "gnu" : "sysv");
    }
  }
  if (!foundDynamicSegment) fail(member, "ELF dynamic segment is missing");
  if (dynamicHashKinds.size === 0) fail(member, "ELF DT_HASH/DT_GNU_HASH entry is missing");
  if (![...dynamicHashKinds].some((kind) => hashKinds.has(kind))) {
    fail(member, "ELF dynamic hash tag has no matching hash section");
  }
  return { dynamicSymbols, hashKinds: [...dynamicHashKinds].sort() };
}

function verifyElf(bytes, member) {
  for (const load of elfLoadSegments(bytes, member)) {
    if (load.offset > bytes.length || load.fileSize > bytes.length - load.offset) {
      fail(member, `ELF LOAD segment ${load.index} extends beyond the shared library`);
    }
    if (load.fileSize > load.memorySize) {
      fail(member, `ELF LOAD segment ${load.index} file size exceeds its memory size`);
    }
    const alignment = BigInt(load.align);
    if (
      load.align < PAGE_SIZE
      || load.align % PAGE_SIZE !== 0
      || (alignment & (alignment - 1n)) !== 0n
    ) {
      fail(member, `ELF LOAD segment ${load.index} alignment ${load.align} is not 16 KiB compatible`);
    }
    if (load.offset % load.align !== load.virtualAddress % load.align) {
      fail(member, `ELF LOAD segment ${load.index} offset and virtual address are not 16 KiB congruent`);
    }
  }
  return elfDynamicShape(bytes, member);
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
  const endDisk = view.getUint16(end + 4, true);
  const centralDirectoryDisk = view.getUint16(end + 6, true);
  const entriesOnDisk = view.getUint16(end + 8, true);
  const entries = view.getUint16(end + 10, true);
  if (endDisk !== 0 || centralDirectoryDisk !== 0) {
    throw new Error("APK: ZIP end-of-central-directory uses unsupported multi-disk metadata");
  }
  if (entriesOnDisk !== entries) {
    throw new Error("APK: ZIP end-of-central-directory entry counts are inconsistent");
  }
  const centralSize = view.getUint32(end + 12, true);
  const centralOffset = view.getUint32(end + 16, true);
  if (centralOffset + centralSize !== end) {
    throw new Error("APK: ZIP central-directory extent is inconsistent");
  }
  const members = [];
  let offset = centralOffset;
  for (let index = 0; index < entries; index += 1) {
    if (offset + 46 > archive.length || view.getUint32(offset, true) !== 0x02014b50) {
      throw new Error("APK: ZIP central-directory record is truncated");
    }
    const flags = view.getUint16(offset + 8, true);
    const method = view.getUint16(offset + 10, true);
    const compressedSize = view.getUint32(offset + 20, true);
    const uncompressedSize = view.getUint32(offset + 24, true);
    const nameLength = view.getUint16(offset + 28, true);
    const extraLength = view.getUint16(offset + 30, true);
    const commentLength = view.getUint16(offset + 32, true);
    const startDisk = view.getUint16(offset + 34, true);
    const localOffset = view.getUint32(offset + 42, true);
    const recordEnd = offset + 46 + nameLength + extraLength + commentLength;
    if (recordEnd > end) {
      throw new Error("APK: ZIP central-directory variable fields are truncated");
    }
    const name = new TextDecoder().decode(archive.subarray(offset + 46, offset + 46 + nameLength));
    if ((flags & 1) !== 0) fail(name, "encrypted ZIP members are unsupported");
    if (startDisk !== 0) fail(name, "ZIP central-directory entry starts on unsupported disk");
    if (localOffset + 30 > archive.length || view.getUint32(localOffset, true) !== 0x04034b50) {
      fail(name, "ZIP local header is missing");
    }
    if ((view.getUint16(localOffset + 6, true) & 1) !== 0) fail(name, "encrypted ZIP members are unsupported");
    const localNameLength = view.getUint16(localOffset + 26, true);
    const localExtraLength = view.getUint16(localOffset + 28, true);
    const dataOffset = localOffset + 30 + localNameLength + localExtraLength;
    if (dataOffset > centralOffset || compressedSize > centralOffset - dataOffset) {
      fail(name, "ZIP member data overlaps the central directory");
    }
    members.push({ name, method, compressedSize, uncompressedSize, dataOffset });
    offset = recordEnd;
  }
  if (offset !== end) throw new Error("APK: ZIP central-directory entry count is inconsistent");
  return members;
}

export function inspectApk(archive, archiveName = "APK") {
  const nativeMembers = zipMembers(archive).filter(({ name }) => name.endsWith(".so"));
  if (nativeMembers.length === 0) {
    throw new Error(`${archiveName}: APK contains no native shared libraries`);
  }
  let declaredNativeBytes = 0;
  for (const member of nativeMembers) {
    if (member.uncompressedSize > MAX_NATIVE_BYTES) {
      fail(
        member.name,
        member.method === 8
          ? "compressed ZIP member exceeds the 512 MiB inspection safety limit"
          : "native library exceeds the 512 MiB loadability limit",
      );
    }
    if (declaredNativeBytes > MAX_NATIVE_BYTES - member.uncompressedSize) {
      fail(
        member.name,
        member.method === 8
          ? "compressed native members exceed the aggregate 512 MiB inspection safety limit"
          : "native libraries exceed the aggregate 512 MiB loadability limit",
      );
    }
    declaredNativeBytes += member.uncompressedSize;
  }
  const measurements = [];
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
        elfBytes = inflateRawSync(stored, { maxOutputLength: member.uncompressedSize + 1 });
      } catch {
        fail(member.name, "compressed ZIP member exceeds its declared size or cannot be decompressed");
      }
      if (elfBytes.length !== member.uncompressedSize) {
        fail(member.name, "compressed ZIP member has inconsistent size");
      }
    } else {
      fail(member.name, `uses unsupported ZIP compression method ${member.method}`);
    }
    const shape = verifyElf(elfBytes, member.name);
    measurements.push({
      member: member.name,
      bytes: elfBytes.length,
      dynamicSymbols: shape.dynamicSymbols,
      hashKinds: shape.hashKinds,
    });
  }
  return measurements;
}

export function verifyApk(archive, archiveName = "APK") {
  return inspectApk(archive, archiveName).length;
}

function main() {
  const apk = process.argv[2];
  if (!apk || process.argv.length !== 3) {
    console.error("Usage: android-verify-16k.mjs <apk>");
    process.exitCode = 2;
    return;
  }
  try {
    const measurements = inspectApk(readFileSync(apk), path.basename(apk));
    const largest = Math.max(...measurements.map(({ bytes }) => bytes));
    const symbols = Math.max(...measurements.map(({ dynamicSymbols }) => dynamicSymbols));
    const hashes = [...new Set(measurements.flatMap(({ hashKinds }) => hashKinds))].sort().join("+");
    console.log(
      `android-verify-16k: PASS apk=${apk} native-libraries=${measurements.length} `
      + `largest-bytes=${largest} max-dynamic-symbols=${symbols} hash=${hashes}`,
    );
  } catch (error) {
    console.error(`android-verify-16k: FAIL ${error.message}`);
    process.exitCode = 1;
  }
}

if (import.meta.main) main();
