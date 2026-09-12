// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

const repository = path.resolve(import.meta.dirname, "../..");
const parser = path.join(repository, "scripts/ci/sccache-backend-error-counts.mjs");
const runner = path.join(repository, "scripts/ci/run-with-sccache-stats.sh");

async function temporaryFile(t, content) {
  const directory = await mkdtemp(path.join(os.tmpdir(), "oxid-sccache-parser-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const file = path.join(directory, "backend-errors.log");
  await writeFile(file, content, { mode: 0o600 });
  return file;
}

function parse(file) {
  return execFileSync("node", [parser, file], { encoding: "utf8" }).trim();
}

test("sccache backend parser emits bounded category counts without source text", async (t) => {
  const secret = "https://token:secret@example.invalid/cache/key";
  const file = await temporaryFile(t, [
    "HTTP 429 rate limited",
    "HTTP 401 unauthorized",
    "HTTP 403 forbidden",
    "HTTP 409 conflict",
    "HTTP 503 service unavailable",
    "transport error: connection reset",
    "timed out waiting for backend",
    `unmatched ${secret}`,
    "",
  ].join("\n"));

  const output = parse(file);
  assert.equal(output, "sccache backend error counts: rate-limit=1 authorization=2 conflict=1 server=1 timeout-transport=2 unknown=1");
  assert.doesNotMatch(output, /429|401|403|409|503|token|example|cache\/key|unmatched/i);
});

test("sccache backend parser classifies empty input as zero fixed categories", async (t) => {
  const file = await temporaryFile(t, "");
  assert.equal(parse(file), "sccache backend error counts: rate-limit=0 authorization=0 conflict=0 server=0 timeout-transport=0 unknown=0");
});

test("trusted writer diagnostics preserve command status, hide raw input, and clean up", async (t) => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "oxid-sccache-runner-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const bin = path.join(directory, "bin");
  await mkdir(bin);
  const sccache = path.join(bin, "sccache");
  const command = path.join(bin, "command-under-test");
  await writeFile(sccache, "#!/usr/bin/env bash\nprintf 'sccache stats\\n'\n", { mode: 0o700 });
  await writeFile(command, [
    "#!/usr/bin/env bash",
    "printf 'raw-command-stderr-sentinel\\n' >&2",
    "if [[ -n \"${SCCACHE_ERROR_LOG:-}\" ]]; then",
    "  printf 'HTTP 429 https://token:secret@example.invalid/cache/key\\n' >>\"$SCCACHE_ERROR_LOG\"",
    "fi",
    "exit \"$1\"",
    "",
  ].join("\n"), { mode: 0o700 });
  await chmod(sccache, 0o700);
  await chmod(command, 0o700);

  for (const status of [0, 17]) {
    const result = spawnSync("bash", [runner, command, String(status)], {
      cwd: repository,
      encoding: "utf8",
      env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, TMPDIR: directory, SCCACHE_BACKEND_DIAGNOSTICS: "on", SCCACHE_GHA_RW_MODE: "READ_WRITE" },
    });
    assert.equal(result.status, status);
    assert.match(result.stdout, /rate-limit=1 authorization=0 conflict=0 server=0 timeout-transport=0 unknown=0/);
    assert.match(result.stderr, /raw-command-stderr-sentinel/);
    assert.doesNotMatch(result.stdout, /raw-command-stderr-sentinel|token|example|cache\/key|HTTP 429/i);
  }

  const leftovers = (await readdir(directory)).filter((name) => name.startsWith("oxid-sccache-backend-errors."));
  assert.deepEqual(leftovers, []);

  const readOnly = spawnSync("bash", [runner, command, "0"], {
    cwd: repository,
    encoding: "utf8",
    env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, TMPDIR: directory, SCCACHE_BACKEND_DIAGNOSTICS: "off", SCCACHE_GHA_RW_MODE: "READ_ONLY" },
  });
  assert.equal(readOnly.status, 0);
  assert.doesNotMatch(readOnly.stdout, /sccache backend error counts/);
  assert.match(readOnly.stderr, /raw-command-stderr-sentinel/);
});

test("workflow grants diagnostics only to the trusted unit writer", async () => {
  const workflow = await readFile(path.join(repository, ".github/workflows/ci.yml"), "utf8");
  const unitJob = workflow.slice(workflow.indexOf("\n  unit_linux:\n"), workflow.indexOf("\n  headless_linux:\n"));
  assert.match(unitJob, /SCCACHE_BACKEND_DIAGNOSTICS', trustedTrainPush \? 'on' : 'off'/);
  assert.equal((workflow.match(/SCCACHE_BACKEND_DIAGNOSTICS/g) || []).length, 1);
});
