#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const [sourcePath, controlOrigin] = process.argv.slice(2);
if (!sourcePath || !path.isAbsolute(sourcePath) || !/^http:\/\/127\.0\.0\.1:18091$/.test(controlOrigin ?? "")) {
  process.stderr.write("portal-mobile-holder-sync: invalid arguments\n");
  process.exit(2);
}

let stopped = false;
let lastDigest = "";
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => { stopped = true; });

while (!stopped) {
  try {
    const bytes = fs.readFileSync(sourcePath);
    const digest = createHash("sha256").update(bytes).digest("hex");
    if (digest !== lastDigest) {
      const response = await fetch(`${controlOrigin}/holder`, {
        method: "POST",
        body: bytes,
        headers: { "Content-Type": "application/json" },
        signal: AbortSignal.timeout(5_000),
      });
      if (response.ok) {
        const result = await response.json();
        lastDigest = digest;
        process.stdout.write(`portal-mobile-holder-sync: public generation=${result.generation}\n`);
      }
    }
  } catch {}
  await new Promise((resolve) => setTimeout(resolve, 200));
}
