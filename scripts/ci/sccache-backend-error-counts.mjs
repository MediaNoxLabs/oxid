#!/usr/bin/env node
import { readFile } from "node:fs/promises";

const categories = ["rate-limit", "authorization", "conflict", "server", "timeout-transport", "unknown"];

function categoryFor(line) {
  if (/\b429\b|rate.limit/iu.test(line)) return "rate-limit";
  if (/\b401\b|\b403\b|unauthori[sz]ed|forbidden|access denied|permission denied/iu.test(line)) return "authorization";
  if (/\b409\b|\bconflict\b/iu.test(line)) return "conflict";
  if (/\b5\d\d\b|server error|service unavailable|bad gateway/iu.test(line)) return "server";
  if (/timed? out|timeout|transport|connection (?:reset|refused)|network|dns|socket|tls|broken pipe/iu.test(line)) return "timeout-transport";
  return "unknown";
}

const counts = Object.fromEntries(categories.map((category) => [category, 0]));
const [file] = process.argv.slice(2);
let input = "";
if (file) {
  try {
    input = await readFile(file, "utf8");
  } catch {
    // A diagnostic read failure must not expose the private file name or alter CI.
  }
}

for (const line of input.split(/\r?\n/u)) {
  if (line) counts[categoryFor(line)] += 1;
}

console.log(`sccache backend error counts: ${categories.map((category) => `${category}=${counts[category]}`).join(" ")}`);
