// SPDX-License-Identifier: Apache-2.0

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import corePreflight, { runDevLoopPreflight as runCorePreflight } from "../../scripts/lib/dev-loop-preflight-core.mjs";

type PreflightRuntime = Parameters<typeof runCorePreflight>[2];

export const runDevLoopPreflight = runCorePreflight;

export default function devLoopPreflight(pi: ExtensionAPI, runtime: PreflightRuntime = {}) {
  return corePreflight(pi, runtime);
}
