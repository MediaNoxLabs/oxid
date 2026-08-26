// SPDX-License-Identifier: Apache-2.0

import {
  checkAgentToolAllowlists,
  devLoopPreflightCacheKey,
  formatAgentToolAllowlistFailure,
  resolveDevLoopsPackageRoot,
} from "./dev-loop-runtime.mjs";

const registeredApis = new WeakSet();

export async function runDevLoopPreflight(pi, cwd, runtime = {}) {
  try {
    const resolve = runtime.resolve ?? resolveDevLoopsPackageRoot;
    const checkAllowlists = runtime.check ?? checkAgentToolAllowlists;
    const cacheKey = runtime.cacheKey ?? devLoopPreflightCacheKey;
    const cache = runtime.cache;
    const resolved = await resolve({ cwd, includeAllPinnedPackages: true });
    const availableTools = pi.getAllTools().map((tool) => tool.name);
    const key = cache ? await cacheKey({ resolved, availableTools }) : undefined;
    if (key !== undefined && cache.has(key)) return cache.get(key);
    const result = await checkAllowlists({
      packageRoot: resolved.packageRoot,
      packageRoots: resolved.packageRoots,
      projectRoot: resolved.gitRoot,
      settings: resolved.settings,
      availableTools,
    });
    const checked = result.ok
      ? { ok: true, kind: "valid" }
      : { ok: false, kind: "invalid-allowlist", message: formatAgentToolAllowlistFailure(result) };
    if (cache && key !== undefined) {
      cache.clear();
      cache.set(key, checked);
    }
    return checked;
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    // Manifest-shape drift in a pinned third-party package must not lock the
    // operator out of the interactive repair turn. Agent/provider launch still
    // fails closed below until the preflight can parse and validate every tool.
    const recoverable = /(?:missing exact |could not read .*package manifest|expected .* CLI at |agent manifest|YAML frontmatter)/i.test(detail);
    return {
      ok: false,
      kind: recoverable ? "unprovisioned" : "invalid-environment",
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
  const check = (cwd) => runDevLoopPreflight(pi, cwd, { ...runtime, cache });

  pi.on("input", async (_event, ctx) => {
    const result = await check(ctx.cwd);
    if (result.ok) return { action: "continue" };
    if (result.kind === "unprovisioned") {
      ctx.ui.notify(`${result.message}. This interactive input is allowed so you can provision or repair the repository; agent/provider launch remains blocked until preflight succeeds.`, "warning");
      return { action: "continue" };
    }
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
