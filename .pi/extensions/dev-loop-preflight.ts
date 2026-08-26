// SPDX-License-Identifier: Apache-2.0

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

import {
  checkAgentToolAllowlists,
  formatAgentToolAllowlistFailure,
  resolveDevLoopsPackageRoot,
} from "../../scripts/lib/dev-loop-runtime.mjs";

type PreflightState = { ok: true } | { ok: false; message: string };

export async function runDevLoopPreflight(pi: ExtensionAPI, cwd: string): Promise<PreflightState> {
  try {
    const resolved = await resolveDevLoopsPackageRoot({ cwd });
    const availableTools = pi.getAllTools().map((tool) => tool.name);
    const result = await checkAgentToolAllowlists({
      packageRoot: resolved.packageRoot,
      settings: resolved.settings,
      availableTools,
    });
    if (!result.ok) return { ok: false, message: formatAgentToolAllowlistFailure(result) };
    return { ok: true };
  } catch (error) {
    return {
      ok: false,
      message: `Pi dev-loop preflight failed before model execution: ${error instanceof Error ? error.message : String(error)}`,
    };
  }
}

export default function devLoopPreflight(pi: ExtensionAPI) {
  // Recheck rather than caching: settings and registered tools can change during
  // a session, including between tool execution and the next provider turn.
  const check = (cwd: string) => runDevLoopPreflight(pi, cwd);

  // Input interception is Pi's documented no-model boundary. It applies to
  // interactive, RPC, and extension-injected user input.
  pi.on("input", async (_event, ctx) => {
    const result = await check(ctx.cwd);
    if (result.ok) return { action: "continue" as const };
    ctx.ui.notify(result.message, "error");
    return { action: "handled" as const };
  });

  // Defense in depth for a model turn started without a user-input event.
  pi.on("before_agent_start", async (_event, ctx) => {
    const result = await check(ctx.cwd);
    if (result.ok) return;
    ctx.ui.notify(result.message, "error");
    ctx.abort();
  });

  // Never permit a request after a failed preflight, even if another extension
  // starts a turn programmatically. Aborting here occurs before HTTP dispatch.
  pi.on("before_provider_request", async (_event, ctx) => {
    const result = await check(ctx.cwd);
    if (result.ok) return;
    ctx.abort();
    throw new Error(result.message);
  });
}
