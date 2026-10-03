// SPDX-License-Identifier: Apache-2.0

export const DEVELOP_BRANCH = "develop";
export const MILESTONE_BRANCH_PATTERN = /^milestone-(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/u;
const DELIVERY_HEADING = /^#{2,3} Delivery target\s*$/gimu;
const OXID_REPOSITORY = "medianoxlabs/oxid";
const GIT_OID = /^[0-9a-f]{40}$/u;

function repositoryFromGithubRemote(remoteUrl) {
  if (typeof remoteUrl !== "string") return null;
  const match = remoteUrl.match(/^(?:https:\/\/github\.com\/|git@github\.com:|ssh:\/\/git@github\.com\/)([A-Za-z0-9_.-]+)\/([A-Za-z0-9_.-]+?)(?:\.git)?\/?$/u);
  return match ? `${match[1]}/${match[2]}`.toLowerCase() : null;
}

function fetchMapsBranch(fetchRefspecs, branch) {
  if (!Array.isArray(fetchRefspecs)) return false;
  const source = `refs/heads/${branch}`;
  const destination = `refs/remotes/origin/${branch}`;
  return fetchRefspecs.some((refspec) => {
    if (typeof refspec !== "string") return false;
    const normalized = refspec.startsWith("+") ? refspec.slice(1) : refspec;
    const [from, to] = normalized.split(":");
    if (!from || !to) return false;
    if (from === source && to === destination) return true;
    return from === "refs/heads/*" && to === "refs/remotes/origin/*";
  });
}

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

/**
 * Prove that a bare issue target and the envelope's origin ref are one pinned
 * delivery target. This is intentionally stricter than textual normalization:
 * repository identity, fetch mapping, ref name, and resolved OID must agree.
 */
export function assertNormalizedDeliveryBase(issueValue, envelopeValue, proof) {
  const issueTarget = parseDeliveryTarget(issueValue);
  const envelopeTarget = parseDeliveryTarget(envelopeValue);
  if (issueTarget.branch !== envelopeTarget.branch) {
    throw new Error(`issue delivery target ${issueTarget.branch} does not match envelope target ${envelopeTarget.branch}`);
  }
  if (proof?.repository?.toLowerCase() !== OXID_REPOSITORY
      || repositoryFromGithubRemote(proof?.originUrl) !== OXID_REPOSITORY) {
    throw new Error("delivery target repository does not match MediaNoxLabs/oxid origin");
  }
  if (proof?.remoteName !== "origin" || !fetchMapsBranch(proof?.fetchRefspecs, issueTarget.branch)) {
    throw new Error(`origin fetch mapping does not resolve ${issueTarget.remoteRef}`);
  }
  if (proof?.resolvedRef !== issueTarget.remoteRef) {
    throw new Error(`resolved delivery ref must be ${issueTarget.remoteRef}`);
  }
  if (!GIT_OID.test(proof?.issueTargetOid ?? "") || !GIT_OID.test(proof?.envelopeTargetOid ?? "")
      || proof.issueTargetOid !== proof.envelopeTargetOid) {
    throw new Error("issue and envelope delivery target OIDs do not match");
  }
  return issueTarget;
}
