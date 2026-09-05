#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const Profile = Object.freeze({
  FEATURE: "feature",
  INTEGRATION: "integration",
  RELEASE: "release",
});

export const DeliveryProfile = Object.freeze({
  PROTOTYPE: "prototype",
  PRODUCTION_READY: "production-ready",
});

export const HostedTarget = Object.freeze({
  BASIC: "basic",
  UNIT_LINUX: "unit-linux",
  HEADLESS_LINUX: "headless-linux",
  UI_LINUX: "ui-linux",
  UI_RELEASE_LINUX: "ui-release-linux",
  COVERAGE_LINUX: "coverage-linux",
  QUALITY: "quality",
  NIX_PACKAGE: "nix-package",
  COMPACT_ARTIFACTS: "compact-artifacts",
});

export const HOSTED_TARGETS = Object.freeze(Object.values(HostedTarget));
const PROTOTYPE_HOSTED_TARGETS = new Set([HostedTarget.UNIT_LINUX, HostedTarget.HEADLESS_LINUX]);
const MILESTONE_BRANCH = /^milestone-(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/u;

const AREA_PATTERNS = Object.freeze({
  docs: [
    /(^|\/)README\.md$/,
    /\.md$/,
    /^docs\//,
    /^scripts\/docs\//,
    /^\.github\/ISSUE_TEMPLATE\//,
    /^(?:LICENSE|CODE_OF_CONDUCT\.md|CONTRIBUTING\.md|SECURITY\.md|SUPPORT\.md)$/,
  ],
  harness: [
    /^\.devloops$/,
    /^\.pi\//,
    /^AGENT\.md$/,
    /^scripts\/check-pi-devshell\.sh$/,
    /^scripts\/(?:dev-loops\.mjs|factory\/|git-hooks\/|github\/|loop\/|lib\/(?:dev-loop|handoff-envelope)|review\/|worktree-lifecycle\.mjs)/,
    /^tests\/repository\//,
  ],
  ci: [
    /^\.github\/(?:actions|workflows)\//,
    /^scripts\/ci\//,
    /^scripts\/coverage\//,
  ],
  build: [
    /^\.cargo\//,
    /^(?:Cargo\.toml|Cargo\.lock|flake\.nix|flake\.lock|run\.sh|rust-toolchain(?:\.toml)?|deny\.toml)$/,
    /^nix\//,
  ],
  compact: [
    /^contracts\//,
    /^tools\/passport-vault-composer\//,
    /^fixtures\/(?:passport-vault|laceid-portal)\//,
    /^scripts\/(?:standalone-|test-standalone-|test-preprod-|derive-preprod-|observe-preprod-)/,
  ],
  headless: [
    /^apps\/(?:oxid-headless|oxid-mcp)\//,
  ],
  ui: [
    /^apps\/oxid\//,
    /^brands\//,
    /^crates\/(?:brand-build|ui-dioxus)\//,
    /^scripts\/(?:check-brand-|check-ui-)/,
  ],
  platform: [
    /^tests\/mobile\//,
    /^crates\/adapters\/(?:mobile-native-plugin|platform-system|storage-mobile)\//,
    /^scripts\/(?:run-android-|run-ios-|test-android-|test-ios-)/,
  ],
  core: [
    /^apps\//,
    /^crates\//,
    /^fixtures\//,
    /^scripts\//,
  ],
});

const BASIC_ONLY_AREAS = new Set(["docs", "harness", "ci"]);

function normalizePath(candidate) {
  return candidate.replaceAll("\\", "/").replace(/^\.\//, "");
}

function matchesAny(candidate, patterns) {
  return patterns.some((pattern) => pattern.test(candidate));
}

export function classifyAreas(paths) {
  const changed = [...new Set(paths.map(normalizePath).filter(Boolean))];
  const areas = new Set();

  for (const candidate of changed) {
    const matches = Object.entries(AREA_PATTERNS)
      .filter(([, patterns]) => matchesAny(candidate, patterns))
      .map(([area]) => area);

    // A source path can be both a focused component and core. Keep only the
    // focused component unless no narrower ownership rule matched it.
    const focused = matches.filter((area) => area !== "core" && area !== "docs");
    if (focused.length > 0) {
      focused.forEach((area) => areas.add(area));
      continue;
    }
    if (matches.includes("docs")) {
      areas.add("docs");
      continue;
    }
    if (matches.includes("core")) {
      areas.add("core");
      continue;
    }

    // Unknown paths fail closed to shared-core validation.
    areas.add("core");
  }

  return [...areas].sort();
}

function featureTargets(areas) {
  const targets = new Set([HostedTarget.BASIC]);
  if (areas.length > 0 && areas.every((area) => BASIC_ONLY_AREAS.has(area))) return targets;

  if (areas.includes("build")) return new Set(HOSTED_TARGETS);

  const rustArea = areas.some((area) => ["core", "headless", "ui", "platform", "compact"].includes(area));
  if (rustArea) {
    targets.add(HostedTarget.UNIT_LINUX);
  }

  if (areas.includes("core") || areas.includes("headless") || areas.includes("compact")) {
    targets.add(HostedTarget.HEADLESS_LINUX);
  }
  if (areas.includes("ui") || areas.includes("platform")) {
    targets.add(HostedTarget.UI_LINUX);
  }
  if (areas.includes("compact")) {
    targets.add(HostedTarget.COMPACT_ARTIFACTS);
  }

  return targets;
}

function orderedTargets(targets) {
  return HOSTED_TARGETS.filter((target) => targets.has(target));
}

export function makeTargetPlan(paths, {
  profile = Profile.FEATURE,
  deliveryProfile = DeliveryProfile.PRODUCTION_READY,
  extraTargets = [],
} = {}) {
  if (!Object.values(Profile).includes(profile)) throw new Error(`unknown CI profile: ${profile}`);
  if (!Object.values(DeliveryProfile).includes(deliveryProfile)) {
    throw new Error(`unknown delivery profile: ${deliveryProfile}`);
  }
  if (deliveryProfile === DeliveryProfile.PROTOTYPE && profile !== Profile.FEATURE) {
    throw new Error("prototype delivery is local-only and supports only the feature CI profile");
  }

  const normalizedPaths = [...new Set(paths.map(normalizePath).filter(Boolean))];
  const diffAvailable = normalizedPaths.length > 0;
  const areas = diffAvailable ? classifyAreas(normalizedPaths) : ["unknown"];
  const targets = deliveryProfile === DeliveryProfile.PROTOTYPE
    ? new Set([HostedTarget.BASIC])
    : !diffAvailable || profile !== Profile.FEATURE
      ? new Set(HOSTED_TARGETS)
      : featureTargets(areas);

  for (const target of extraTargets) {
    if (!HOSTED_TARGETS.includes(target)) throw new Error(`unknown hosted CI target: ${target}`);
    if (deliveryProfile === DeliveryProfile.PROTOTYPE && !PROTOTYPE_HOSTED_TARGETS.has(target)) {
      throw new Error(`hosted CI target is not available in prototype delivery: ${target}`);
    }
    targets.add(target);
  }

  return {
    deliveryProfile,
    profile,
    diffAvailable,
    areas,
    targets: orderedTargets(targets),
    rustChanged: !diffAvailable
      || areas.some((area) => ["build", "compact", "core", "headless", "platform", "ui"].includes(area)),
  };
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
    const output = execFileSync("git", ["diff", "--no-renames", "--name-only", "-z", base, head, "--"], {
      cwd,
      encoding: "utf8",
      maxBuffer: 16 * 1024 * 1024,
    });
    return output.split("\0").filter(Boolean);
  } catch {
    process.stderr.write(`[target-plan] could not compare ${base}..${head}; selecting all hosted targets\n`);
    return null;
  }
}

export function resolveProfile(requested, eventName, baseBranch = "", refName = "", headBranch = "") {
  if (requested && requested !== "auto") return requested;
  if (eventName === "pull_request") {
    if (baseBranch === "main") return Profile.RELEASE;
    if (baseBranch === "develop" && MILESTONE_BRANCH.test(headBranch)) return Profile.INTEGRATION;
    return Profile.FEATURE;
  }
  if (eventName === "push") return refName === "main" ? Profile.RELEASE : Profile.INTEGRATION;
  return Profile.RELEASE;
}

function parseTargets(value) {
  return (value ?? "").split(",").map((target) => target.trim()).filter(Boolean);
}

function githubOutput(plan) {
  const selected = new Set(plan.targets);
  const lines = [
    `delivery_profile=${plan.deliveryProfile}`,
    `profile=${plan.profile}`,
    `areas=${plan.areas.join(",")}`,
    `targets=${plan.targets.join(",")}`,
    `rust_changed=${plan.rustChanged}`,
  ];
  for (const target of HOSTED_TARGETS) {
    lines.push(`${target.replaceAll("-", "_")}=${selected.has(target)}`);
  }
  return `${lines.join("\n")}\n`;
}

export function run(argv = process.argv.slice(2), { cwd = process.cwd(), stdout = process.stdout } = {}) {
  const profile = resolveProfile(
    readOption(argv, "--profile") ?? "auto",
    readOption(argv, "--event"),
    readOption(argv, "--base-branch"),
    readOption(argv, "--ref-name"),
    readOption(argv, "--head-branch"),
  );
  const deliveryProfile = readOption(argv, "--delivery-profile") ?? DeliveryProfile.PRODUCTION_READY;
  const paths = changedPaths(readOption(argv, "--base"), readOption(argv, "--head"), cwd);
  const plan = makeTargetPlan(paths ?? [], {
    profile,
    deliveryProfile,
    extraTargets: parseTargets(readOption(argv, "--targets")),
  });
  const format = readOption(argv, "--format") ?? "summary";

  if (format === "github") stdout.write(githubOutput(plan));
  else if (format === "json") stdout.write(`${JSON.stringify(plan, null, 2)}\n`);
  else stdout.write(`${plan.deliveryProfile}/${plan.profile}: ${plan.targets.join(", ")} [${plan.areas.join(", ")}]\n`);
  return plan;
}

const directPath = process.argv[1] ? path.resolve(process.argv[1]) : "";
if (directPath === fileURLToPath(import.meta.url)) run();
