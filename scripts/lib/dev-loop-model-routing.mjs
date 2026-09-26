// SPDX-License-Identifier: Apache-2.0

const THINKING_LEVELS = new Set(["off", "minimal", "low", "medium", "high", "xhigh", "max"]);
const MODEL_PART = /^[a-z0-9][a-z0-9.-]*$/u;

export function resolveSupervisorModelRoute(model, thinking) {
  const provider = typeof model?.provider === "string" ? model.provider.trim() : "";
  const id = typeof model?.id === "string" ? model.id.trim() : "";
  const effort = typeof thinking === "string" ? thinking.trim() : "";
  if (!MODEL_PART.test(provider) || !MODEL_PART.test(id)) {
    throw new Error("active supervisor model must be a well-formed provider/model pair");
  }
  if (!THINKING_LEVELS.has(effort)) {
    throw new Error(`active supervisor thinking level is unsupported: ${JSON.stringify(thinking)}`);
  }
  return Object.freeze({
    provider,
    id,
    thinking: effort,
    model: `${provider}/${id}`,
    routedModel: `${provider}/${id}:${effort}`,
  });
}

export function inspectDevLoopDispatch({ toolName, input, model, thinking }) {
  if (toolName !== "subagent" || input?.agent !== "dev-loop") return { applies: false, block: false };
  let route;
  try {
    route = resolveSupervisorModelRoute(model, thinking);
  } catch (error) {
    return {
      applies: true,
      block: true,
      reason: `Dev-loop model routing failed closed before dispatch: ${error.message}`,
    };
  }
  if (input.model !== route.routedModel) {
    return {
      applies: true,
      block: true,
      route,
      reason: `Dev-loop child must use the active supervisor route ${route.routedModel}; received ${JSON.stringify(input.model ?? null)}.`,
    };
  }
  return { applies: true, block: false, route };
}

export function devLoopRoutingInstruction(model, thinking) {
  const route = resolveSupervisorModelRoute(model, thinking);
  return {
    route,
    text: [
      "Supervisor model routing is fail-closed for the sole dev-loop implementation child.",
      `Dispatch agent dev-loop with the exact per-run model ${route.routedModel}.`,
      "Do not omit, replace, downgrade, or retry that route; the pre-dispatch guard rejects disagreement.",
    ].join(" "),
  };
}
