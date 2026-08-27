// SPDX-License-Identifier: Apache-2.0

import {
  checkAgentToolAllowlists,
  devLoopPreflightCacheKey,
  formatAgentToolAllowlistFailure,
  PI_BUILTIN_CHILD_TOOLS,
  resolveDevLoopsPackageRoot,
} from "./dev-loop-runtime.mjs";

const registeredApis = new WeakSet();

function toolNames(tools) {
  if (!Array.isArray(tools)) return [];
  return tools.flatMap((tool) => {
    if (typeof tool === "string") return tool.trim() ? [tool.trim()] : [];
    return typeof tool?.name === "string" && tool.name.trim() ? [tool.name.trim()] : [];
  });
}

function resolveToolScopes(pi, runtime) {
  const availableTools = toolNames(pi.getAllTools());
  const env = runtime.env ?? process.env;
  const activeAgent = runtime.activeAgent ?? env.PI_SUBAGENT_CHILD_AGENT;
  const activeTools = toolNames(runtime.activeTools ?? pi.getActiveTools?.() ?? availableTools);
  const depth = Number(env.PI_SUBAGENT_DEPTH);
  const maximumDepth = Number(env.PI_SUBAGENT_MAX_DEPTH);
  const canDispatchChild = availableTools.includes("subagent")
    || (env.PI_SUBAGENT_CHILD === "1" && Number.isInteger(depth) && Number.isInteger(maximumDepth) && depth < maximumDepth);
  const futureTools = [...new Set([...PI_BUILTIN_CHILD_TOOLS, ...availableTools, ...(canDispatchChild ? ["subagent"] : [])])];
  return { availableTools, activeAgent, activeTools, futureTools };
}

export async function runDevLoopPreflight(pi, cwd, runtime = {}) {
  try {
    const resolve = runtime.resolve ?? resolveDevLoopsPackageRoot;
    const checkAllowlists = runtime.check ?? checkAgentToolAllowlists;
    const cacheKey = runtime.cacheKey ?? devLoopPreflightCacheKey;
    const cache = runtime.cache;
    const resolved = await resolve({ cwd, includeAllPinnedPackages: true });
    const scopes = resolveToolScopes(pi, runtime);
    const key = cache ? await cacheKey({ resolved, ...scopes }) : undefined;
    if (key !== undefined && cache.has(key)) return cache.get(key);
    const result = await checkAllowlists({
      packageRoot: resolved.packageRoot,
      packageRoots: resolved.packageRoots,
      projectRoot: resolved.gitRoot,
      settings: resolved.settings,
      ...scopes,
    });
    const checked = result.ok
      ? { ok: true }
      : { ok: false, message: formatAgentToolAllowlistFailure(result) };
    if (cache && key !== undefined) {
      cache.clear();
      cache.set(key, checked);
    }
    return checked;
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    return {
      ok: false,
      message: `Pi dev-loop preflight environment is not ready: ${detail}`,
    };
  }
}

export default function devLoopPreflight(pi, runtime = {}) {
  // Pi may evaluate an adapter more than once during extension reload. Never
  // duplicate the three guards for the same ExtensionAPI instance.
  if (registeredApis.has(pi)) return;
  registeredApis.add(pi);
  const cache = runtime.cache ?? new Map();
  const check = (cwd, scopes = {}) => runDevLoopPreflight(pi, cwd, { ...runtime, ...scopes, cache });

  pi.on("input", async (_event, ctx) => {
    const result = await check(ctx.cwd);
    if (result.ok) return { action: "continue" };
    ctx.ui.notify(`${result.message}. Pi 0.84 hooks cannot cancel agent or provider execution; diagnose here, then run the tracked pre-flight wrapper before any routed action or delegation.`, "warning");
    return { action: "continue" };
  });

  pi.on("before_agent_start", async (event, ctx) => {
    const result = await check(ctx.cwd, { activeTools: event?.systemPromptOptions?.selectedTools });
    if (!result.ok) ctx.ui.notify(`${result.message}. Advisory only: Pi 0.84 before_agent_start has no cancellation result.`, "error");
  });

  pi.on("before_provider_request", async (_event, ctx) => {
    const result = await check(ctx.cwd, { activeTools: toolNames(pi.getActiveTools?.() ?? []) });
    if (!result.ok) ctx.ui.notify(`${result.message}. Advisory only: Pi 0.84 before_provider_request errors are swallowed by the runner.`, "error");
  });
}
