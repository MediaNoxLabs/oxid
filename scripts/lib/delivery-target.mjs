// SPDX-License-Identifier: Apache-2.0

export const DEVELOP_BRANCH = "develop";
export const MILESTONE_BRANCH_PATTERN = /^milestone-(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/u;
const DELIVERY_HEADING = /^#{2,3} Delivery target\s*$/gimu;

export function parseDeliveryTarget(value) {
  if (typeof value !== "string" || value.length === 0 || value !== value.trim()) {
    throw new Error("delivery target must be an exact non-empty branch or origin branch ref");
  }
  const branch = value.startsWith("origin/") ? value.slice("origin/".length) : value;
  if (branch !== DEVELOP_BRANCH && !MILESTONE_BRANCH_PATTERN.test(branch)) {
    throw new Error("delivery target must be develop or milestone-<x.y.z>");
  }
  return Object.freeze({
    branch,
    remoteRef: `origin/${branch}`,
    kind: branch === DEVELOP_BRANCH ? "factory" : "milestone",
  });
}

export function deliveryTargetFromIssueBody(body) {
  if (typeof body !== "string") throw new Error("issue body is unavailable");
  const headings = [...body.matchAll(DELIVERY_HEADING)];
  if (headings.length !== 1) throw new Error("issue must contain exactly one '## Delivery target' heading");
  const following = body.slice(headings[0].index + headings[0][0].length);
  const section = following.split(/^#{2,3}\s+/mu, 1)[0];
  const values = section.split(/\r?\n/u)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => line.replace(/^`([^`]+)`$/u, "$1"));
  if (values.length !== 1) throw new Error("Delivery target section must contain exactly one branch name");
  return parseDeliveryTarget(values[0]);
}

export function extractDeliveryTargetOption(argv, { required = false } = {}) {
  const args = [];
  const values = [];
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--delivery-base") {
      const value = argv[index + 1];
      if (!value || value.startsWith("-")) throw new Error("--delivery-base requires a value");
      values.push(value);
      index += 1;
    } else if (argument.startsWith("--delivery-base=")) {
      const value = argument.slice("--delivery-base=".length);
      if (!value) throw new Error("--delivery-base requires a value");
      values.push(value);
    } else {
      args.push(argument);
    }
  }
  if (values.length > 1) throw new Error("--delivery-base may be specified only once");
  if (required && values.length === 0) {
    throw new Error("--delivery-base is required; use the exact target recorded on the issue");
  }
  return { args, target: values.length === 1 ? parseDeliveryTarget(values[0]) : null };
}

export function assertIssueTarget(issueBody, expected) {
  const recorded = deliveryTargetFromIssueBody(issueBody);
  const selected = typeof expected === "string" ? parseDeliveryTarget(expected) : expected;
  if (!selected || recorded.branch !== selected.branch) {
    throw new Error(`issue delivery target ${recorded.branch} does not match selected target ${selected?.branch ?? "none"}`);
  }
  return recorded;
}
