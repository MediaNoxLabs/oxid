#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const ChangeTier = Object.freeze({
  DOCS: "docs",
  HARNESS: "harness",
  RUST: "rust",
  FULL: "full",
});

const DOCUMENT_PATHS = [
  /(^|\/)README\.md$/,
  /\.md$/,
  /^docs\//,
  /^\.github\/ISSUE_TEMPLATE\//,
  /^(?:LICENSE|CODE_OF_CONDUCT\.md|CONTRIBUTING\.md|SECURITY\.md|SUPPORT\.md)$/,
];

const HARNESS_PATHS = [
  /^\.devloops$/,
  /^\.pi\//,
  /^AGENT\.md$/,
  /^docs\/factory\//,
  /^docs\/dev-loop-stability\.md$/,
  /^scripts\/dev-loops\.mjs$/,
  /^scripts\/github\//,
  /^scripts\/loop\//,
  /^scripts\/lib\/(?:dev-loop|handoff-envelope)/,
  /^scripts\/review\//,
  /^scripts\/ci\//,
  /^tests\/repository\/(?:dev-loop|change-tier)/,
];

const FULL_PATHS = [
  /^\.github\/(?:actions|workflows)\//,
  /^(?:flake\.nix|flake\.lock|Cargo\.lock|rust-toolchain(?:\.toml)?|deny\.toml)$/,
  /^nix\//,
  /^contracts\//,
  /^scripts\/check-(?:architecture|midnight-sources|advisories)\.sh$/,
  /^crates\/(?:credential|identity|passport-vault|presentation|protocol|wallet)\//,
  /^crates\/adapters\/(?:backup-[^/]+|custody-software|did-midnight|identity-ingress|midnight|mobile-native-plugin|openid4v(?:ci|p)|passport-vault|siopv2|storage-(?:credential|identity|mobile)|store-atomic|vc-midnight)(?:\/|$)/,
];

function matchesAny(candidate, patterns) {
  return patterns.some((pattern) => pattern.test(candidate));
}

function normalizePath(candidate) {
  return candidate.replaceAll("\\", "/").replace(/^\.\//, "");
}

/**
 * Return the cheapest safe CI tier for a set of repository-relative paths.
 * Unknown and mixed source changes deliberately fall back to Rust validation;
 * sensitive build, workflow, custody, identity, and protocol surfaces run full.
 */
export function classifyChangedPaths(paths) {
  const changed = [...new Set(paths.map(normalizePath).filter(Boolean))];
  if (changed.length === 0) return ChangeTier.FULL;
  if (changed.some((candidate) => matchesAny(candidate, FULL_PATHS))) return ChangeTier.FULL;
  if (changed.every((candidate) => matchesAny(candidate, DOCUMENT_PATHS))) return ChangeTier.DOCS;
  if (changed.every((candidate) => (
    matchesAny(candidate, DOCUMENT_PATHS) || matchesAny(candidate, HARNESS_PATHS)
  ))) return ChangeTier.HARNESS;
  return ChangeTier.RUST;
}

function readOption(argv, name) {
  const index = argv.indexOf(name);
  if (index === -1) return undefined;
  return argv[index + 1];
}

function changedPaths(base, head, cwd) {
  if (!base || !head || /^0+$/.test(base)) return null;
  try {
    for (const ref of [base, head]) {
      execFileSync("git", ["rev-parse", "--verify", `${ref}^{commit}`], { cwd, stdio: "ignore" });
    }
    // Disable rename folding so moving a sensitive file to a low-risk path
    // still classifies both the deletion and addition.
    const output = execFileSync("git", ["diff", "--no-renames", "--name-only", "-z", base, head, "--"], {
      cwd,
      encoding: "utf8",
      maxBuffer: 16 * 1024 * 1024,
    });
    return output.split("\0").filter(Boolean);
  } catch (error) {
    process.stderr.write(`[change-tier] could not compare ${base}..${head}; selecting full validation\n`);
    return null;
  }
}

export function run(argv = process.argv.slice(2), { cwd = process.cwd(), stdout = process.stdout } = {}) {
  const base = readOption(argv, "--base");
  const head = readOption(argv, "--head");
  const paths = changedPaths(base, head, cwd);
  const tier = paths === null ? ChangeTier.FULL : classifyChangedPaths(paths);
  stdout.write(`${tier}\n`);
  return tier;
}

const directPath = process.argv[1] ? path.resolve(process.argv[1]) : "";
if (directPath === fileURLToPath(import.meta.url)) run();
