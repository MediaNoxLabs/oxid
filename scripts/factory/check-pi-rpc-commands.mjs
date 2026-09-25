// SPDX-License-Identifier: Apache-2.0

import { parseArgs } from "node:util";
import { pathToFileURL } from "node:url";

export const PI_RPC_MAX_BYTES = 1024 * 1024;

export async function readBoundedPiRpcInput(stream, { maxBytes = PI_RPC_MAX_BYTES } = {}) {
  const chunks = [];
  let bytes = 0;
  for await (const chunk of stream) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    bytes += buffer.length;
    if (bytes > maxBytes) {
      throw new Error(`Pi RPC command discovery exceeded the ${maxBytes}-byte limit`);
    }
    chunks.push(buffer);
  }
  return Buffer.concat(chunks, bytes).toString("utf8");
}

export function validatePiRpcCommandDiscovery(source, { loaderPath } = {}) {
  if (typeof loaderPath !== "string" || loaderPath.length === 0) {
    throw new Error("Pi RPC command discovery requires one loader path");
  }
  const messages = String(source).split("\n").filter((line) => line.trim().length > 0).map((line, index) => {
    try {
      return JSON.parse(line);
    } catch (error) {
      throw new Error(`Pi RPC command discovery returned malformed JSON on line ${index + 1}`, { cause: error });
    }
  });
  const response = messages.find((entry) => entry?.type === "response" && entry?.command === "get_commands");
  const commands = response?.data?.commands;
  if (!Array.isArray(commands)) {
    throw new Error("Pi RPC command discovery did not return a get_commands response");
  }
  if (commands.some(({ name }) => name === "tf" || name === "skill:taskflow")) {
    throw new Error("unsafe inherited taskflow resources are active; project suppression did not take effect");
  }
  const names = new Set(commands.map(({ name }) => name));
  if (!names.has("scenario") || !names.has("use-case")) {
    throw new Error("Pi did not expose the tracked scenario and use-case commands");
  }
  const reviewSkill = commands.some((command) => (
    command?.name === "skill:agent-review"
    && command?.source === "skill"
    && command?.sourceInfo?.path === loaderPath
  ));
  if (!reviewSkill) {
    throw new Error("Pi did not expose the bundled agent-review 0.6.0 skill");
  }
  return { commandCount: commands.length };
}

export function validatePiModelCatalog(source, { provider, model } = {}) {
  if (typeof provider !== "string" || provider.length === 0 || typeof model !== "string" || model.length === 0) {
    throw new Error("Pi model catalog validation requires one provider and model");
  }
  const found = String(source).split("\n").some((line) => {
    const [candidateProvider, candidateModel] = line.trim().split(/\s+/u);
    return candidateProvider === provider && candidateModel === model;
  });
  if (!found) {
    throw new Error(`tracked Pi model is absent from the Nix-pinned catalog: ${provider}/${model}`);
  }
  return { provider, model };
}

export async function main(argv = process.argv.slice(2), { stdin = process.stdin } = {}) {
  const { values } = parseArgs({
    args: argv,
    options: {
      "loader-path": { type: "string" },
      provider: { type: "string" },
      model: { type: "string" },
    },
    strict: true,
  });
  const source = await readBoundedPiRpcInput(stdin);
  if (values.provider !== undefined || values.model !== undefined) {
    validatePiModelCatalog(source, { provider: values.provider, model: values.model });
    return;
  }
  validatePiRpcCommandDiscovery(source, { loaderPath: values["loader-path"] });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    process.stderr.write(`[pi-rpc-command-check] ${error.message}\n`);
    process.exitCode = 1;
  });
}
