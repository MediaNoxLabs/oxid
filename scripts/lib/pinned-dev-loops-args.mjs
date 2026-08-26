// SPDX-License-Identifier: Apache-2.0

/**
 * Argument shapes supported by the exact dev-loops@0.9.0 repository pin.
 * A pin upgrade must update this table and its contract tests before wrappers
 * accept newly introduced global options.
 */
const GLOBAL_VALUE_OPTIONS = new Set(["--jq", "--repo", "--cwd", "--config"]);
const GLOBAL_BOOLEAN_OPTIONS = new Set(["--silent", "-s", "--json"]);

export function readLongOptionValues(args, name) {
  const values = [];
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === name) {
      if (index + 1 >= args.length || args[index + 1].startsWith("-")) throw new Error(`${name} requires a value`);
      values.push(args[index + 1]);
      index += 1;
    } else if (argument.startsWith(`${name}=`)) {
      const value = argument.slice(name.length + 1);
      if (!value) throw new Error(`${name} requires a value`);
      values.push(value);
    }
  }
  return values;
}

export function pinnedPublicRoute(args) {
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (GLOBAL_BOOLEAN_OPTIONS.has(argument)) continue;
    const equalsOption = [...GLOBAL_VALUE_OPTIONS].find((option) => argument.startsWith(`${option}=`));
    if (equalsOption) {
      if (argument.length === equalsOption.length + 1) throw new Error(`${equalsOption} requires a value`);
      continue;
    }
    if (GLOBAL_VALUE_OPTIONS.has(argument)) {
      if (index + 1 >= args.length || args[index + 1].startsWith("-")) throw new Error(`${argument} requires a value`);
      index += 1;
      continue;
    }
    if (argument.startsWith("-")) {
      throw new Error(`unsupported leading dev-loops@0.9.0 option: ${argument}; update the pinned wrapper contract before a pin upgrade`);
    }
    return { category: argument, command: args[index + 1] };
  }
  return {};
}

export function enforceSingleBase(args, requiredBase, { addWhenMissing = false, label = "repository operation" } = {}) {
  const bases = readLongOptionValues(args, "--base");
  if (bases.length > 1) throw new Error(`${label} accepts exactly one base`);
  if (bases.some((base) => base !== requiredBase)) throw new Error(`${label} must use ${requiredBase}`);
  if (addWhenMissing && bases.length === 0) return [...args, "--base", requiredBase];
  return [...args];
}
