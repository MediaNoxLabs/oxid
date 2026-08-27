// SPDX-License-Identifier: Apache-2.0

const SHA256 = /^[a-f0-9]{64}$/;
const HEAD_SHA = /^[a-f0-9]{40}$/;
const PROVENANCE_LABEL = /^[A-Za-z0-9_.:-]{1,64}$/;
export const MAX_REPAIR_EVIDENCE_AGE_MS = 30 * 60 * 1000;

export function validateFanoutRepairEvidence({ existing, requested, provenance, nowMs = Date.now() }) {
  if (!existing || existing.visible !== true || existing.contractComplete !== true) {
    throw new Error("repair requires an existing contract-complete gate marker");
  }
  if (!HEAD_SHA.test(requested?.headSha ?? "") || requested.headSha !== existing.headSha) {
    throw new Error("repair requires the exact current 40-character head SHA");
  }
  if (!new Set(["clean", "findings"]).has(requested.verdict)) throw new Error("repair verdict must be clean or findings");
  if (existing.verdict === "findings" && requested.verdict === "clean") {
    throw new Error("evidence repair cannot turn findings into clean");
  }
  if (existing.executionMode === "fanout_fanin") {
    return { action: "noop", reason: "current-head fanout evidence is already present" };
  }
  if (existing.executionMode !== "inline_single_agent") {
    throw new Error("only inline_single_agent evidence can be upgraded");
  }
  if (!provenance || provenance.schemaVersion !== 1 || provenance.gate !== requested.gate || provenance.headSha !== requested.headSha) {
    throw new Error("invalid fanout provenance identity");
  }
  const generatedAtMs = Date.parse(provenance.generatedAt ?? "");
  if (!Number.isFinite(generatedAtMs) || generatedAtMs > nowMs || nowMs - generatedAtMs > MAX_REPAIR_EVIDENCE_AGE_MS) {
    throw new Error("fanout provenance is stale or has an invalid generatedAt");
  }
  if (!Array.isArray(provenance.reviewers) || provenance.reviewers.length < 2) {
    throw new Error("fanout provenance requires at least two distinct reviewers");
  }
  const reviewerIds = new Set();
  const angles = new Set();
  let findingCount = 0;
  const findings = [];
  for (const reviewer of provenance.reviewers) {
    if (!PROVENANCE_LABEL.test(reviewer?.reviewerId ?? "") || reviewerIds.has(reviewer.reviewerId)) {
      throw new Error("fanout provenance reviewer identities must be non-empty and distinct");
    }
    if (!PROVENANCE_LABEL.test(reviewer.angle ?? "") || angles.has(reviewer.angle)) {
      throw new Error("fanout provenance angles must be non-empty and distinct");
    }
    if (!SHA256.test(reviewer.artifactSha256 ?? "")) throw new Error("fanout provenance artifact digest is invalid");
    const completedAtMs = Date.parse(reviewer.completedAt ?? "");
    if (!Number.isFinite(completedAtMs) || completedAtMs > generatedAtMs || nowMs - completedAtMs > MAX_REPAIR_EVIDENCE_AGE_MS) {
      throw new Error("fanout reviewer evidence is stale or has an invalid completedAt");
    }
    if (!new Set(["clean", "findings"]).has(reviewer.verdict)) throw new Error("fanout reviewer verdict is invalid");
    if (!Array.isArray(reviewer.findings)) throw new Error("fanout reviewer findings must be an array");
    if (reviewer.verdict === "clean" && reviewer.findings.length > 0) throw new Error("clean fanout reviewer evidence cannot contain findings");
    if (reviewer.verdict === "findings" && reviewer.findings.length === 0) throw new Error("findings fanout reviewer evidence must contain findings");
    for (const finding of reviewer.findings) {
      if (!/^[a-z][a-z0-9-]{0,31}$/.test(finding?.severity ?? "")) throw new Error("fanout finding severity is invalid");
      if (typeof finding.summary !== "string" || finding.summary.trim() === "" || finding.summary.length > 500 || /[\r\n\0]/.test(finding.summary)) {
        throw new Error("fanout finding summary is invalid");
      }
      findings.push({ angle: reviewer.angle, severity: finding.severity, summary: finding.summary.trim() });
    }
    findingCount += reviewer.findings.length;
    reviewerIds.add(reviewer.reviewerId);
    angles.add(reviewer.angle);
  }
  if (requested.verdict === "clean" && findingCount > 0) throw new Error("evidence repair cannot classify fanout findings as clean");
  if (requested.verdict === "findings" && findingCount === 0) throw new Error("findings repair requires recorded fanout findings");
  return {
    action: "upgrade",
    executionMode: "fanout_fanin",
    verdict: requested.verdict,
    headSha: requested.headSha,
    reviewerCount: reviewerIds.size,
    angles: [...angles],
    findingCount,
    findings,
    audit: {
      fromExecutionMode: existing.executionMode,
      existingCommentId: existing.commentId,
      repairedAt: new Date(nowMs).toISOString(),
      provenanceGeneratedAt: provenance.generatedAt,
    },
  };
}
