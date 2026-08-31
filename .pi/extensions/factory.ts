/**
 * AI Software Factory — read-only Pi status extension.
 *
 * GitHub mutations remain in tracked, contract-tested repository wrappers.
 * The historical claim skeleton changed assignees, labels, and comments in
 * three non-atomic commands and did not implement the documented lease race
 * protocol. Keep the command visible, but fail closed until a guarded claim
 * wrapper owns that complete transaction.
 */
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const REPO = "MediaNoxLabs/oxid";

async function gh(pi: ExtensionAPI, args: string[]): Promise<string> {
  const result = await pi.exec("gh", args);
  if (result.exitCode !== 0) {
    throw new Error(`gh ${args.join(" ")} failed: ${result.stderr}`);
  }
  return result.stdout;
}

export default function (pi: ExtensionAPI) {
  pi.registerCommand("factory", {
    description:
      "AI Software Factory: read-only `backlog`/`status`; `claim` fails closed (docs/factory)",
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
          ctx.ui.notify(
            `Claiming #${issue} is disabled: the former extension was not atomic and bypassed the guarded lease/branch policy. ` +
              "Use an issue-backed {type}/issue-N branch through the tracked factory workflow.",
            "error",
          );
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
