// SPDX-License-Identifier: Apache-2.0

import { closeSync, createReadStream, mkdtempSync, openSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";

import { runManagedChild } from "./managed-child-process.mjs";

function parse(argv) {
  const split = argv.indexOf("--");
  if (split < 0 || split === argv.length - 1) throw new Error("missing command");
  const flags = argv.slice(0, split);
  const read = (name) => {
    const index = flags.indexOf(name);
    if (index < 0 || index === flags.length - 1 || flags.indexOf(name, index + 1) >= 0) throw new Error(`missing ${name}`);
    return flags[index + 1];
  };
  const timeoutSeconds = Number(read("--timeout-seconds"));
  const label = read("--label");
  if (!Number.isFinite(timeoutSeconds) || timeoutSeconds <= 0 || timeoutSeconds > 7200 || !/^[a-z0-9][a-z0-9-]{0,79}$/u.test(label)) {
    throw new Error("invalid managed command options");
  }
  const command = argv.slice(split + 1);
  return { timeoutMs: Math.ceil(timeoutSeconds * 1000), label, command: command[0], args: command.slice(1) };
}

export async function main(argv = process.argv.slice(2)) {
  const options = parse(argv);
  const privateDir = mkdtempSync(path.join(os.tmpdir(), "oxid-managed-command."));
  const stdoutPath = path.join(privateDir, "stdout");
  const stderrPath = path.join(privateDir, "stderr");
  let stdoutFd = openSync(stdoutPath, "wx", 0o600);
  let stderrFd = openSync(stderrPath, "wx", 0o600);
  try {
    let code;
    let failure;
    try {
      code = await runManagedChild(options.command, options.args, {
        cwd: process.cwd(),
        env: process.env,
        label: options.label,
        timeoutMs: options.timeoutMs,
        stdio: ["inherit", stdoutFd, stderrFd],
      });
    } catch (error) {
      failure = error;
    }
    closeSync(stdoutFd);
    closeSync(stderrFd);
    stdoutFd = -1;
    stderrFd = -1;
    await Promise.all([
      pipeline(createReadStream(stdoutPath), process.stdout, { end: false }),
      pipeline(createReadStream(stderrPath), process.stderr, { end: false }),
    ]);
    if (failure) throw failure;
    return code;
  } finally {
    if (stdoutFd >= 0) closeSync(stdoutFd);
    if (stderrFd >= 0) closeSync(stderrFd);
    rmSync(privateDir, { recursive: true, force: true });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().then((code) => { process.exitCode = code; }, (error) => {
    process.stderr.write(`managed-command: ${error.message}\n`);
    process.exitCode = 1;
  });
}
