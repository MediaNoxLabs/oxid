// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import test from "node:test";

import {
  editIssue,
  parseEditIssueArgs,
  runCli,
} from "../../scripts/github/edit-issue.mjs";

test("the sanctioned issue-claim facade accepts only the exact @me claim mutation", () => {
  assert.deepEqual(
    parseEditIssueArgs(["--repo", "medianoxlabs/oxid", "--issue", "471", "--add-assignee", "@me"]),
    { repository: "medianoxlabs/oxid", issue: 471, addAssignee: "@me" },
  );
  assert.throws(
    () => parseEditIssueArgs(["--repo", "medianoxlabs/oxid", "--issue", "471", "--add-assignee", "octocat"]),
    /only supports `--add-assignee @me`/u,
  );
  assert.throws(
    () => parseEditIssueArgs(["--repo", "invalid", "--issue", "471", "--add-assignee", "@me"]),
    /OWNER\/REPO/u,
  );
  assert.throws(
    () => parseEditIssueArgs(["--repo", "medianoxlabs/oxid", "--issue", "0", "--add-assignee", "@me"]),
    /positive integer/u,
  );
  assert.throws(
    () => parseEditIssueArgs(["--repo", "medianoxlabs/oxid", "--issue", "471"]),
    /only supports `--add-assignee @me`/u,
  );
  assert.throws(
    () => parseEditIssueArgs(["--repo", "medianoxlabs/oxid", "--issue", "471", "--remove-assignee", "@me"]),
    /Unknown option/u,
  );
});

test("the facade resolves the authenticated login and adds it without replacing assignees", () => {
  const calls = [];
  const result = editIssue({
    repository: "medianoxlabs/oxid",
    issue: 471,
    addAssignee: "@me",
    runGh: (args) => {
      calls.push(args);
      return calls.length === 1 ? '{"login":"oxid-dev"}' : '{"number":471}';
    },
  });
  assert.deepEqual(result, { ok: true, repository: "medianoxlabs/oxid", issue: 471, assignee: "oxid-dev" });
  assert.equal(calls[0].at(-1), "user");
  assert.deepEqual(calls[1].slice(-3), ["repos/medianoxlabs/oxid/issues/471/assignees", "-f", "assignees[]=oxid-dev"]);
  assert.ok(calls[1].includes("--method"));
  assert.ok(calls[1].includes("POST"));
  assert.ok(!calls[1].some((value) => String(value).includes("issues/471") && !String(value).endsWith("/assignees")), "must use the additive assignee endpoint");
});

test("the facade fails closed when authentication or the REST mutation fails", () => {
  assert.throws(
    () => editIssue({ repository: "medianoxlabs/oxid", issue: 471, addAssignee: "@me", runGh: () => { throw new Error("authentication required"); } }),
    /authentication required/u,
  );
  assert.throws(
    () => editIssue({
      repository: "medianoxlabs/oxid",
      issue: 471,
      addAssignee: "@me",
      runGh: (args) => args.at(-1) === "user" ? "{}" : "{}",
    }),
    /authenticated GitHub login/u,
  );
  assert.throws(
    () => editIssue({
      repository: "medianoxlabs/oxid",
      issue: 471,
      addAssignee: "@me",
      runGh: (args) => args.at(-1) === "user" ? '{"login":"oxid-dev"}' : (() => { throw new Error("API failure"); })(),
    }),
    /API failure/u,
  );
});

test("the command exposes help and prints machine-readable success", () => {
  let output = "";
  runCli(["--help"], { stdout: { write: (value) => { output += value; } } });
  assert.match(output, /--add-assignee @me/u);

  output = "";
  runCli(["--repo", "medianoxlabs/oxid", "--issue", "471", "--add-assignee", "@me"], {
    stdout: { write: (value) => { output += value; } },
    runGh: (args) => args.at(-1) === "user" ? '{"login":"oxid-dev"}' : "{}",
  });
  assert.deepEqual(JSON.parse(output), { ok: true, repository: "medianoxlabs/oxid", issue: 471, assignee: "oxid-dev" });
});
