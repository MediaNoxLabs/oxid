// SPDX-License-Identifier: Apache-2.0

const REQUIRED_SECTIONS = [
  "Implementation surface",
  "AC / DoD matrix",
  "Verification",
  "Size",
  "Delivery target",
  "Non-goals",
];

function section(body, heading) {
  const pattern = new RegExp(`^##\\s+${heading.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&")}\\s*\\n([\\s\\S]*?)(?=^##\\s+|(?![\\s\\S]))`, "mu");
  return body.match(pattern)?.[1].trim() ?? "";
}

/** Validate the machine-actionable contract required before factory admission. */
export function validateFactoryIssueContract({ title, body }) {
  const errors = [];
  if (typeof title !== "string" || !title.trim()) errors.push("missing issue title");
  if (typeof body !== "string") return { ok: false, errors: [...errors, "missing issue body"], sections: {} };

  const sections = Object.fromEntries(REQUIRED_SECTIONS.map((heading) => [heading, section(body, heading)]));
  for (const heading of REQUIRED_SECTIONS) {
    if (!sections[heading]) errors.push(`missing ${heading}`);
  }

  const matrixRows = sections["AC / DoD matrix"].split(/\r?\n/u)
    .map((line) => line.split("|").slice(1, -1).map((cell) => cell.trim()))
    .filter(([criterion]) => /^AC-\d+\b/u.test(criterion ?? ""));
  const acceptanceIds = [];
  for (const [criterion, evidence] of matrixRows) {
    const match = criterion.match(/^(AC-\d+)\s*:\s*(.+)$/u);
    if (!match || !/[A-Za-z]{3,}/u.test(match[2])) {
      errors.push("AC / DoD matrix criteria must include a stable AC-<n> identifier and a concrete outcome");
      continue;
    }
    acceptanceIds.push(match[1]);
    if (typeof evidence !== "string" || !/[A-Za-z]{3,}/u.test(evidence)) {
      errors.push(`AC / DoD matrix lacks concrete completion evidence for ${match[1]}`);
    }
  }
  const uniqueIds = [...new Set(acceptanceIds)];
  if (uniqueIds.length === 0) errors.push("AC / DoD matrix must map stable AC-<n> criteria to concrete completion evidence");
  if (uniqueIds.length !== acceptanceIds.length) errors.push("AC / DoD matrix contains duplicate stable identifiers");

  if (!/^(?:S|M|L|Small|Medium|Large)\b/imu.test(sections.Size)) errors.push("size must be S, M, or L");
  if (!/^(?:develop|milestone-\d+\.\d+\.\d+)\s*$/imu.test(sections["Delivery target"])) {
    errors.push("delivery target must be exactly develop or milestone-x.y.z");
  }
  return { ok: errors.length === 0, errors, sections, acceptanceIds: uniqueIds };
}

export function assertFactoryIssueContract(issue) {
  const result = validateFactoryIssueContract(issue);
  if (!result.ok) throw new Error(`factory issue contract is incomplete: ${result.errors.join("; ")}`);
  return result;
}
