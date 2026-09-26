// SPDX-License-Identifier: Apache-2.0

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import corePreflight, { runDevLoopPreflight as runCorePreflight } from "../../scripts/lib/dev-loop-preflight-core.mjs";
import {
  devLoopRoutingInstruction,
  inspectDevLoopDispatch,
} from "../../scripts/lib/dev-loop-model-routing.mjs";

type PreflightRuntime = Parameters<typeof runCorePreflight>[2];

export const runDevLoopPreflight = runCorePreflight;

export default function devLoopPreflight(pi: ExtensionAPI, runtime: PreflightRuntime = {}) {
  corePreflight(pi, runtime);

  pi.on("before_agent_start", (event, ctx) => {
    try {
      const { text } = devLoopRoutingInstruction(ctx.model, ctx.thinkingLevel);
      return { systemPrompt: `${event.systemPrompt}\n\n${text}` };
    } catch (error) {
      ctx.ui.notify(`Dev-loop model routing is unavailable: ${error instanceof Error ? error.message : String(error)}`, "error");
    }
  });

  const onRuntimeEvent = pi.on as unknown as (
    event: string,
    handler: (event: { toolName?: string; input?: unknown }, ctx: { model?: { provider?: string; id?: string }; thinkingLevel?: string }) => unknown,
  ) => void;
  onRuntimeEvent("tool_call", (event, ctx) => {
    const decision = inspectDevLoopDispatch({
      toolName: event.toolName,
      input: event.input,
      model: ctx.model,
      thinking: ctx.thinkingLevel,
    });
    if (decision.block) return { block: true, reason: decision.reason };
  });
}
