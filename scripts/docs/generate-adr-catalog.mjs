#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { realpathSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

export const INTEGRATION_ADR_BLOB_BASE = "https://github.com/MediaNoxLabs/oxid/blob/integration/docs/adr";

export function renderAdrCatalog(index) {
  const body = index
    .replace(/^# /gm, "## ")
    .replace(/\]\((([0-9]{4})[A-Za-z0-9./_-]*\.md)\)/g, `](${INTEGRATION_ADR_BLOB_BASE}/$1)`);
  return [
    "# Decision records",
    "",
    `> Regenerated at build time from [\`docs/adr/README.md\`](${INTEGRATION_ADR_BLOB_BASE}/README.md).`,
    "",
    body,
  ].join("\n");
}

function parseArgs(argv) {
  const values = {};
  for (let index = 0; index < argv.length; index += 2) {
    const name = argv[index];
    const value = argv[index + 1];
    if ((name !== "--index" && name !== "--output") || !value) {
      throw new Error("usage: generate-adr-catalog.mjs --index <path> --output <path>");
    }
    values[name.slice(2)] = value;
  }
  if (!values.index || !values.output) {
    throw new Error("usage: generate-adr-catalog.mjs --index <path> --output <path>");
  }
  return values;
}

export async function main(argv = process.argv.slice(2)) {
  const values = parseArgs(argv);
  const index = await readFile(path.resolve(values.index), "utf8");
  await writeFile(path.resolve(values.output), renderAdrCatalog(index));
}

if (process.argv[1] && realpathSync(path.resolve(process.argv[1])) === realpathSync(fileURLToPath(import.meta.url))) {
  main().catch((error) => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
