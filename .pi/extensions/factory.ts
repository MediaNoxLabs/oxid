/**
 * AI Software Factory — pi repo-local extension (M2 skeleton).
 *
 * Gives any pi session in this repository three commands that wrap `gh`, so
 * factory participation works from any machine with a GitHub login and needs
 * no coordination server. Protocol: docs/factory/ (issue #35).
 *
 * Status: SKELETON for review. Command surfaces and lease format are the
 * contract; error handling and pagination are deliberately minimal until the
 * protocol docs are accepted.
 */
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const REPO = "MediaNoxLabs/oxid";
const LEASE_TTL_HOURS = 4;

async function gh(pi: ExtensionAPI, args: string[]): Promise<string> {
  const result = await pi.exec("gh", args);
  if (result.exitCode !== 0) {
    throw new Error(`gh ${args.join(" ")} failed: ${result.stderr}`);
  }
  return result.stdout;
}

function leaseBody(worker: string, branch: string): string {
  const now = new Date();
  const expires = new Date(now.getTime() + LEASE_TTL_HOURS * 3600 * 1000);
  return [
    "```yaml",
    "factory-lease:",
    `  worker: "${worker}"`,
    `  claimed_at: "${now.toISOString()}"`,
    `  expires_at: "${expires.toISOString()}"`,
    `  branch: "${branch}"`,
    "  renewal: 0",
    "```",
  ].join("\n");
}

export default function (pi: ExtensionAPI) {
  pi.registerCommand("factory", {
    description:
      "AI Software Factory: `backlog` | `claim <issue>` | `status` (docs/factory)",
    handler: async (args, ctx) => {
      const [sub, issueArg] = (args ?? "").trim().split(/\s+/);
      switch (sub) {
        case "backlog": {
          const out = await gh(pi, [
            "issue", "list", "-R", REPO,
            "--label", "factory:ready",
            "--json", "number,title,labels,updatedAt",
            "--jq",
            '.[] | "#\\(.number) \\(.title) (updated \\(.updatedAt))"',
          ]);
          ctx.ui.notify(out.trim() || "No factory:ready items.", "info");
          return;
        }
        case "claim": {
          const issue = Number(issueArg);
          if (!Number.isInteger(issue) || issue <= 0) {
            ctx.ui.notify("Usage: /factory claim <issue-number>", "error");
            return;
          }
          // Guard: refuse when an assignee already exists (cheap double-claim check;
          // the lease-timestamp tiebreak in docs/factory/claim-protocol.md still applies).
          const assignees = await gh(pi, [
            "issue", "view", String(issue), "-R", REPO,
            "--json", "assignees", "--jq", ".assignees | length",
          ]);
          if (assignees.trim() !== "0") {
            ctx.ui.notify(`#${issue} already has an assignee; not claiming.`, "error");
            return;
          }
          const worker = `pi/${process.env.USER ?? "unknown"}`;
          const branch = `factory/${issue}-wip`;
          await gh(pi, ["issue", "edit", String(issue), "-R", REPO, "--add-assignee", "@me"]);
          await gh(pi, ["issue", "comment", String(issue), "-R", REPO, "--body", leaseBody(worker, branch)]);
          await gh(pi, [
            "issue", "edit", String(issue), "-R", REPO,
            "--remove-label", "factory:ready", "--add-label", "factory:claimed",
          ]);
          ctx.ui.notify(`Claimed #${issue} as ${worker}; lease ${LEASE_TTL_HOURS}h; branch ${branch}.`, "info");
          return;
        }
        case "status": {
          const out = await gh(pi, [
            "issue", "list", "-R", REPO,
            "--assignee", "@me", "--state", "open",
            "--json", "number,title,labels",
            "--jq",
            '.[] | "#\\(.number) [\\([.labels[].name | select(startswith("factory:"))] | join(","))] \\(.title)"',
          ]);
          ctx.ui.notify(out.trim() || "No claimed factory items.", "info");
          return;
        }
        default:
          ctx.ui.notify("Usage: /factory backlog | claim <issue> | status", "info");
      }
    },
  });
}
