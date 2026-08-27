// SPDX-License-Identifier: Apache-2.0

export function resolveCanonicalReviewRoute({ maxCopilotRounds, mandatoryAngles = [], copilotAvailable }) {
  if (!Number.isInteger(maxCopilotRounds) || maxCopilotRounds < 0) {
    throw new Error("maxCopilotRounds must be a non-negative integer");
  }
  if (!Array.isArray(mandatoryAngles) || mandatoryAngles.some((angle) => typeof angle !== "string" || angle.trim() === "")) {
    throw new Error("mandatoryAngles must be non-empty strings");
  }
  const externalRequired = mandatoryAngles.includes("external-review");
  if (maxCopilotRounds === 0 || copilotAvailable === false) {
    if (!externalRequired) {
      throw new Error("Copilot is disabled or unavailable but external-review is not mandatory");
    }
    return {
      route: "external-review",
      action: "run_independent_current_head_review",
      preservesRequiredReview: true,
    };
  }
  return {
    route: "copilot",
    action: "request_copilot_review",
    preservesRequiredReview: externalRequired,
  };
}
