// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  stat,
  symlink,
  writeFile,
} from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";
import test from "node:test";

import {
  applyGitHooks,
  BUNDLE_FILES,
  checkGitHooks,
  HOOK_NAMES,
  inspectManagedHookBundle,
} from "../../scripts/git-hooks/configure.mjs";
import {
  inspectSigningConfiguration,
  parsePushUpdates,
  planPushRanges,
  validateMessageFile,
  validatePrePush,
} from "../../scripts/git-hooks/local-policy.mjs";
import { verifyOpenPgpCommit } from "../../scripts/ci/contribution-policy.mjs";
import { withManagedHookWarningFilter } from "../../scripts/loop/ensure-worktree-consumer.mjs";

const repoRoot = path.resolve(import.meta.dirname, "../..");

function git(repository, args) {
  return execFileSync("git", args, { cwd: repository, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

async function fixture(t) {
  const repository = await mkdtemp(path.join(os.tmpdir(), "oxid-hooks-"));
  t.after(() => rm(repository, { recursive: true, force: true }));
  git(repository, ["init", "--initial-branch=integration"]);
  git(repository, ["config", "user.name", "Factory Agent"]);
  git(repository, ["config", "user.email", "agent@example.com"]);
  git(repository, ["config", "user.signingkey", "DEADBEEF"]);
  await mkdir(path.join(repository, ".githooks"));
  for (const name of HOOK_NAMES) {
    await copyFile(path.join(repoRoot, ".githooks", name), path.join(repository, ".githooks", name));
  }
  for (const relative of BUNDLE_FILES) {
    const destination = path.join(repository, relative);
    await mkdir(path.dirname(destination), { recursive: true });
    await copyFile(path.join(repoRoot, relative), destination);
  }
  return repository;
}

test("repository-local hook installation is stable in Git-common private state", async (t) => {
  const repository = await fixture(t);
  const installed = applyGitHooks(repository, { execute: true });
  assert.equal(installed.ok, true, installed.errors.join("; "));
  assert.equal(git(repository, ["config", "--local", "core.hooksPath"]), installed.installedDir);
  assert.equal(git(repository, ["config", "--local", "commit.gpgSign"]), "true");
  assert.equal(git(repository, ["config", "--local", "gpg.format"]), "openpgp");
  for (const name of HOOK_NAMES) {
    const installedHook = path.join(installed.installedDir, name);
    assert.deepEqual(await readFile(installedHook), await readFile(path.join(repository, ".githooks", name)));
    assert.notEqual((await stat(installedHook)).mode & 0o111, 0);
  }
  for (const relative of BUNDLE_FILES) {
    assert.deepEqual(
      await readFile(path.join(installed.bundleDir, relative)),
      await readFile(path.join(repository, relative)),
    );
  }
  assert.equal(checkGitHooks(repository).ok, true);
});

test("canonical factory bundle is recognized without inventing a pre-merge dispatcher", async (t) => {
  const repository = await fixture(t);
  applyGitHooks(repository, { execute: true });
  const initial = inspectManagedHookBundle(repository);
  assert.equal(initial.ok, true);
  git(repository, ["config", "--local", "core.hooksPath", path.join(initial.installedDir, "..", "hooks")]);
  assert.equal(inspectManagedHookBundle(repository).ok, true);
  assert.equal(checkGitHooks(repository).ok, true);
  applyGitHooks(repository, { execute: true });
  assert.equal(initial.preMergeCommitRequired, false);

  const stale = path.join(initial.installedDir, "pre-commit");
  await writeFile(stale, `${await readFile(stale, "utf8")}# stale\n`);
  const staleResult = inspectManagedHookBundle(repository);
  assert.equal(staleResult.managed, true);
  assert.equal(staleResult.ok, false);
  assert.equal(staleResult.stale, true);
  assert.match(staleResult.reason, /explicit bootstrap repair/u);
  applyGitHooks(repository, { execute: true });
  assert.equal(inspectManagedHookBundle(repository).ok, true);
});

test("consumer suppresses only pinned false warnings while canonical hooks remain valid", async (t) => {
  const repository = await fixture(t);
  const installed = applyGitHooks(repository, { execute: true });
  const output = [];
  const stderr = new Writable({ write(chunk, _encoding, callback) { output.push(chunk.toString()); callback(); } });
  const defaultWarning = `[ensure-worktree] WARN default-branch guard not installed: core.hooksPath is set to "${installed.installedDir}" — install the guard there, or unset it, or use DEVLOOPS_ALLOW_MAIN discipline instead\n`;
  const commitWarning = `[ensure-worktree] WARN commit-msg guard not installed: core.hooksPath is set to "${installed.installedDir}" — install the guard there, or unset it\n`;

  const filtered = await withManagedHookWarningFilter(repository, async () => {
    process.stderr.write(defaultWarning);
    process.stderr.write("retained warning\n");
    process.stderr.write(commitWarning);
    return 42;
  }, { stderr });

  assert.equal(filtered.value, 42);
  assert.equal(filtered.suppressed, 2);
  assert.equal(output.join(""), "retained warning\n");

  output.length = 0;
  const invalidated = await withManagedHookWarningFilter(repository, async () => {
    process.stderr.write(defaultWarning);
    await rm(path.join(installed.installedDir, "pre-push"));
    process.stderr.write(commitWarning);
  }, { stderr });
  assert.equal(invalidated.managed.ok, false);
  assert.equal(invalidated.suppressed, 0);
  assert.equal(output.join(""), `${defaultWarning}${commitWarning}`);
});

test("canonical incomplete and symlink-escaped bundles are never recognized", async (t) => {
  const repository = await fixture(t);
  const installed = applyGitHooks(repository, { execute: true });
  await rm(path.join(installed.installedDir, "pre-push"));
  assert.equal(inspectManagedHookBundle(repository).managed, false);

  await rm(installed.installedDir, { recursive: true, force: true });
  const escaped = path.join(repository, "escaped-hooks");
  await mkdir(escaped);
  await symlink(escaped, installed.installedDir);
  assert.equal(inspectManagedHookBundle(repository).managed, false);
});

test("canonical factory bundle is shared truthfully by linked worktrees", async (t) => {
  const repository = await fixture(t);
  await writeFile(path.join(repository, "tracked"), "base\n");
  git(repository, ["add", "tracked", ".githooks", "scripts", ".github"]);
  git(repository, ["commit", "-m", "test(harness): establish fixture"]);
  applyGitHooks(repository, { execute: true });
  const linked = path.join(repository, "linked");
  git(repository, ["worktree", "add", "-b", "fix/issue-431", linked]);
  t.after(() => { try { git(repository, ["worktree", "remove", "--force", linked]); } catch {} });
  const main = inspectManagedHookBundle(repository);
  const fromLinked = inspectManagedHookBundle(linked);
  assert.equal(fromLinked.ok, true);
  assert.equal(fromLinked.installedDir, main.installedDir);
  assert.equal(checkGitHooks(linked).ok, true);
});

test("installer preserves a foreign hook manager", async (t) => {
  const repository = await fixture(t);
  git(repository, ["config", "--local", "core.hooksPath", "/private/other-hooks"]);
  assert.throws(() => applyGitHooks(repository, { execute: true }), /refusing to replace another hook manager/u);
  assert.equal(git(repository, ["config", "--local", "core.hooksPath"]), "/private/other-hooks");
});

test("pre-commit policy requires OpenPGP signing defaults and exact identity inputs", async (t) => {
  const repository = await fixture(t);
  git(repository, ["config", "--local", "commit.gpgSign", "true"]);
  git(repository, ["config", "--local", "gpg.format", "openpgp"]);
  assert.equal(inspectSigningConfiguration(repository).ok, true);
  git(repository, ["config", "--local", "commit.gpgSign", "false"]);
  assert.match(inspectSigningConfiguration(repository).errors.join("\n"), /commit\.gpgSign must be true/u);
});

test("commit-msg validates Conventional Commit and exact DCO before commit creation", async (t) => {
  const repository = await fixture(t);
  const message = path.join(repository, "COMMIT_EDITMSG");
  await writeFile(message, "feat(factory): add local policy hooks\n\nSigned-off-by: Factory Agent <agent@example.com>\n");
  assert.equal(validateMessageFile(repository, message).ok, true);
  await writeFile(message, "feat: missing scope\n\nSigned-off-by: Factory Agent <other@example.com>\n");
  const invalid = validateMessageFile(repository, message);
  assert.equal(invalid.ok, false);
  assert.match(invalid.errors.join("\n"), /subject must match/u);
  assert.match(invalid.errors.join("\n"), /missing exact DCO trailer/u);
});

test("pre-push plans the complete issue range from each branch's recorded target", () => {
  const localSha = "a".repeat(40);
  const remoteSha = "b".repeat(40);
  const baseSha = "c".repeat(40);
  const input = `refs/heads/feat/issue-200 ${localSha} refs/heads/feat/issue-200 ${remoteSha}\n`;
  const calls = [];
  const gitRunner = (_repository, args) => {
    calls.push(args);
    if (args[0] === "merge-base") return baseSha;
    return args.at(-1);
  };
  const deliveryBaseResolver = () => "refs/remotes/origin/milestone-0.4.0";
  assert.deepEqual(planPushRanges({ repository: "/repo", remoteName: "origin", input, gitRunner, deliveryBaseResolver }), [{
    branch: "feat/issue-200",
    deliveryRef: "refs/remotes/origin/milestone-0.4.0",
    deliveryBranch: "milestone-0.4.0",
    base: baseSha,
    head: localSha,
    remoteRef: "refs/heads/feat/issue-200",
  }]);
  assert.ok(calls.some((args) => args.includes("refs/remotes/origin/milestone-0.4.0^{commit}")));

  const zeros = "0".repeat(40);
  assert.deepEqual(planPushRanges({
    repository: "/repo",
    remoteName: "origin",
    input: `(delete) ${zeros} refs/heads/feat/issue-200 ${remoteSha}\n`,
    gitRunner,
    deliveryBaseResolver,
  }), []);
  assert.equal(parsePushUpdates(input).length, 1);
  assert.throws(() => planPushRanges({
    repository: "/repo",
    remoteName: "origin",
    input: `refs/tags/v1 ${localSha} refs/tags/v1 ${remoteSha}\n`,
    gitRunner,
    deliveryBaseResolver,
  }), /branch pushes only/u);
  assert.throws(() => planPushRanges({
    repository: "/repo",
    remoteName: "origin",
    input: `refs/heads/feat/issue-200 ${localSha} refs/heads/develop ${remoteSha}\n`,
    gitRunner,
    deliveryBaseResolver,
  }), /only push to the same remote ref/u);

  const headRunner = (_repository, args) => {
    if (args[0] === "symbolic-ref") return "refs/heads/feat/issue-200";
    if (args[0] === "merge-base") return baseSha;
    return args.at(-1);
  };
  assert.equal(planPushRanges({
    repository: "/repo",
    remoteName: "origin",
    input: `HEAD ${localSha} refs/heads/feat/issue-200 ${remoteSha}\n`,
    gitRunner: headRunner,
    deliveryBaseResolver,
  })[0].branch, "feat/issue-200");
});

test("pre-push requests local cryptographic verification and propagates range failures", () => {
  const localSha = "a".repeat(40);
  const remoteSha = "b".repeat(40);
  const baseSha = "c".repeat(40);
  const input = `refs/heads/fix/issue-200 ${localSha} refs/heads/fix/issue-200 ${remoteSha}\n`;
  const gitRunner = (_repository, args) => args[0] === "merge-base" ? baseSha : args.at(-1);
  let options;
  const failed = validatePrePush({
    repository: "/repo",
    remoteName: "origin",
    input,
    gitRunner,
    deliveryBaseResolver: () => "refs/remotes/origin/develop",
    rangeValidator(candidate) {
      options = candidate;
      return { ok: false, errors: [], commits: [{ commit: localSha, ok: false, errors: ["invalid signature"] }] };
    },
  });
  assert.equal(options.verifyOpenPgp, true);
  assert.equal(options.base, baseSha);
  assert.equal(options.baseRef, "develop");
  assert.equal(options.headRef, "fix/issue-200");
  assert.equal(failed.ok, false);
});

test("local OpenPGP verifier rejects an unsigned commit", async (t) => {
  const repository = await fixture(t);
  await writeFile(path.join(repository, "README.md"), "fixture\n");
  git(repository, ["add", "README.md"]);
  git(repository, ["-c", "commit.gpgSign=false", "commit", "--no-verify", "--no-gpg-sign", "-s", "-m", "feat(factory): create unsigned fixture"]);
  assert.match(verifyOpenPgpCommit(repository, "HEAD"), /cryptographic verification failed/u);
});

test("tracked hook dispatchers are executable and load the stable installed policy bundle", async () => {
  for (const name of HOOK_NAMES) {
    const file = path.join(repoRoot, ".githooks", name);
    const contents = await readFile(file, "utf8");
    assert.notEqual((await stat(file)).mode & 0o111, 0);
    assert.match(contents, /hook_dir=.*dirname/u);
    assert.match(contents, /policy-root\/scripts\/git-hooks\/local-policy\.mjs/u);
  }
});
