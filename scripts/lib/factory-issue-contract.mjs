// SPDX-License-Identifier: Apache-2.0

const REQUIRED_SECTIONS = [
  "Implementation surface",
  "Acceptance criteria",
  "Definition of done / evidence mapping",
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

  const acceptanceIds = [...sections["Acceptance criteria"].matchAll(/\bAC-(\d+)\b/gu)].map((match) => `AC-${match[1]}`);
  const uniqueIds = [...new Set(acceptanceIds)];
  if (uniqueIds.length === 0) errors.push("acceptance criteria must use stable AC-<n> identifiers");
  if (uniqueIds.length !== acceptanceIds.length) errors.push("acceptance criteria contain duplicate stable identifiers");
  const evidenceIds = new Set(sections["Definition of done / evidence mapping"]
    .split(/\r?\n/u)
    .map((line) => line.split("|")[1]?.trim())
    .filter((value) => /^AC-\d+$/u.test(value)));
  for (const id of uniqueIds) {
    if (!evidenceIds.has(id)) errors.push(`definition of done lacks evidence mapping for ${id}`);
  }

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
