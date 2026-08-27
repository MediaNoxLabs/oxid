// SPDX-License-Identifier: Apache-2.0

import { spawn } from "node:child_process";

const SIGNAL_EXIT_CODE = Object.freeze({ SIGHUP: 129, SIGINT: 130, SIGTERM: 143 });
const FORWARDED_SIGNALS = Object.keys(SIGNAL_EXIT_CODE);

export function signalProcessTree(pid, signal, {
  platform = process.platform,
  kill = process.kill.bind(process),
} = {}) {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  const target = platform === "win32" ? pid : -pid;
  try {
    kill(target, signal);
    return true;
  } catch (error) {
    if (error?.code === "ESRCH") return false;
    throw error;
  }
}

/**
 * Spawn one owned process group. Parent interruption terminates the complete
 * group, waits a bounded grace period, then escalates to SIGKILL.
 */
export function runManagedChild(command, args, {
  cwd,
  env,
  stdout = process.stdout,
  stderr = process.stderr,
  label = "child process",
  graceMs = 3000,
  spawnImpl = spawn,
  processRef = process,
  platform = process.platform,
  kill = process.kill.bind(process),
} = {}) {
  return new Promise((resolve, reject) => {
    const child = spawnImpl(command, args, {
      cwd,
      env,
      detached: platform !== "win32",
      stdio: ["inherit", "pipe", "pipe"],
    });
    child.stdout?.pipe(stdout, { end: false });
    child.stderr?.pipe(stderr, { end: false });

    let parentSignal;
    let escalation;
    const send = (signal) => signalProcessTree(child.pid, signal, { platform, kill });
    const handlers = new Map(FORWARDED_SIGNALS.map((signal) => [signal, () => {
      if (parentSignal) return;
      parentSignal = signal;
      send("SIGTERM");
      escalation = setTimeout(() => send("SIGKILL"), graceMs);
      escalation.unref?.();
    }]));
    const onExit = () => send("SIGKILL");

    for (const [signal, handler] of handlers) processRef.once(signal, handler);
    processRef.once("exit", onExit);

    const cleanup = () => {
      if (escalation) clearTimeout(escalation);
      for (const [signal, handler] of handlers) processRef.off(signal, handler);
      processRef.off("exit", onExit);
    };
    child.once("error", (error) => {
      cleanup();
      reject(error);
    });
    child.once("close", (code, signal) => {
      cleanup();
      if (parentSignal) resolve(SIGNAL_EXIT_CODE[parentSignal]);
      else if (signal) reject(new Error(`${label} terminated by ${signal}`));
      else resolve(code ?? 1);
    });
  });
}
