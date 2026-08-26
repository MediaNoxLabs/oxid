// SPDX-License-Identifier: Apache-2.0

import {
  checkAgentToolAllowlists,
  formatAgentToolAllowlistFailure,
  resolveDevLoopsPackageRoot,
} from "../../scripts/lib/dev-loop-runtime.mjs";

export async function runDevLoopPreflight(pi, cwd, runtime = {}) {
  try {
    const resolve = runtime.resolve ?? resolveDevLoopsPackageRoot;
    const checkAllowlists = runtime.check ?? checkAgentToolAllowlists;
    const resolved = await resolve({ cwd });
    const availableTools = pi.getAllTools().map((tool) => tool.name);
    const result = await checkAllowlists({
      packageRoot: resolved.packageRoot,
      projectRoot: resolved.gitRoot,
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

export default function devLoopPreflight(pi, runtime = {}) {
  const check = (cwd) => runDevLoopPreflight(pi, cwd, runtime);

  pi.on("input", async (_event, ctx) => {
    const result = await check(ctx.cwd);
    if (result.ok) return { action: "continue" };
    ctx.ui.notify(result.message, "error");
    return { action: "handled" };
  });

  pi.on("before_agent_start", async (_event, ctx) => {
    const result = await check(ctx.cwd);
    if (result.ok) return;
    ctx.ui.notify(result.message, "error");
    ctx.abort();
  });

  pi.on("before_provider_request", async (_event, ctx) => {
    const result = await check(ctx.cwd);
    if (result.ok) return;
    ctx.abort();
    throw new Error(result.message);
  });
}
