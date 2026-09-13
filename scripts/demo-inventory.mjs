#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const defaultInventoryPath = path.join(repoRoot, "docs/factory/demo-inventory.json");
const inventorySchemaPath = path.join(repoRoot, "docs/factory/demo-inventory.schema.json");
const cadences = new Set(["per-change", "weekly", "on-demand", "before-release", "manual"]);
const evidenceClasses = new Set(["preflight", "diagnostic", "acceptance", "planned"]);
const commandPhases = new Set(["preparation", "build", "deploy", "run", "status", "cleanup"]);
const commandStatuses = new Set(["unsupported", "manual", "delegated"]);
const targetStatuses = new Set(["supported", "unsupported"]);
const targetSupport = new Set(["development-only", "manual-acceptance", "diagnostic-only", "unsupported"]);
const dependencyKinds = new Set(["device", "simulator", "service", "network", "tool"]);
const environmentValues = new Map([
  ["OXID_MOBILE_CUSTODY", new Set(["development", "native"])],
  ["OXID_UI_PROFILE", new Set(["user", "dev", "demo"])],
  ["OXID_STANDALONE_NETWORK_PROFILE", new Set(["simulated", "local", "tailnet"])],
]);

function fail(message) { throw new Error(message); }
function array(value, label) { if (!Array.isArray(value)) fail(`${label} must be an array`); return value; }
function text(value, label) { if (typeof value !== "string" || value.trim() === "") fail(`${label} must be a non-empty string`); return value; }
function idMap(items, label) {
  const result = new Map();
  for (const item of array(items, label)) {
    const id = text(item?.id, `${label} id`);
    if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(id)) fail(`${label} has invalid id '${id}'`);
    if (result.has(id)) fail(`${label} has duplicate id '${id}'`);
    result.set(id, item);
  }
  return result;
}
function references(ids, known, label) {
  const seen = new Set();
  for (const value of array(ids, label)) {
    const id = text(value, `${label} id`);
    if (seen.has(id)) fail(`${label} contains duplicate id '${id}'`);
    if (!known.has(id)) fail(`${label} references unknown id '${id}'`);
    seen.add(id);
  }
}
function safeRelativePath(value, label) {
  text(value, label);
  if (path.isAbsolute(value) || value.includes("\\") || value.split("/").includes("..")) fail(`${label} must be a safe repository-relative path`);
  if (!existsSync(path.join(repoRoot, value))) fail(`${label} does not exist: '${value}'`);
}
function safeCommand(command, label) {
  text(command, label);
  if (command.includes("\n") || /(?:^|\s)\/|[;&|`$<>]/u.test(command) || command.split(/\s+/u).some((part) => part.includes(".."))) fail(`${label} is unsafe or contains an absolute command`);
  const allowed = [
    /^just [a-z0-9][a-z0-9-]*(?: [a-zA-Z0-9_.-]+)*$/u,
    /^node scripts\/[a-zA-Z0-9_./-]+(?: [a-zA-Z0-9_.-]+)*$/u,
    /^\.\/scripts\/[a-zA-Z0-9_./-]+(?: [a-zA-Z0-9_.-]+)*$/u,
    /^adb devices$/u,
  ];
  if (!allowed.some((pattern) => pattern.test(command))) fail(`${label} is not a supported repository command`);
}
function safeEnvironment(environment, label) {
  if (environment === undefined) return;
  if (!environment || typeof environment !== "object" || Array.isArray(environment)) fail(`${label} must be an object`);
  for (const [name, value] of Object.entries(environment)) {
    const values = environmentValues.get(name);
    if (!values || !values.has(value)) fail(`${label} has unsupported ${name} value`);
  }
}
function schemaValue(schema, root) {
  if (!schema.$ref) return schema;
  const parts = schema.$ref.replace(/^#\//u, "").split("/");
  return parts.reduce((value, part) => value?.[part], root);
}
function sameValue(left, right) { return JSON.stringify(left) === JSON.stringify(right); }
function validateSchema(value, schema, root, label = "inventory") {
  const rule = schemaValue(schema, root);
  if (!rule) fail(`${label} has an unresolved schema reference`);
  if (rule.type === "object" && (!value || typeof value !== "object" || Array.isArray(value))) fail(`${label} violates schema type object`);
  if (rule.type === "array" && !Array.isArray(value)) fail(`${label} violates schema type array`);
  if (rule.type === "string" && typeof value !== "string") fail(`${label} violates schema type string`);
  if (rule.type === "boolean" && typeof value !== "boolean") fail(`${label} violates schema type boolean`);
  if (rule.const !== undefined && !sameValue(value, rule.const)) fail(`${label} violates schema const`);
  if (rule.enum && !rule.enum.some((candidate) => sameValue(value, candidate))) fail(`${label} violates schema enum`);
  if (rule.minLength !== undefined && value.length < rule.minLength) fail(`${label} violates schema minLength`);
  if (rule.pattern && !new RegExp(rule.pattern, "u").test(value)) fail(`${label} violates schema pattern`);
  if (Array.isArray(value)) {
    if (rule.minItems !== undefined && value.length < rule.minItems) fail(`${label} violates schema minItems`);
    if (rule.uniqueItems && new Set(value.map((item) => JSON.stringify(item))).size !== value.length) fail(`${label} violates schema uniqueItems`);
    if (rule.items) value.forEach((item, index) => validateSchema(item, rule.items, root, `${label}[${index}]`));
  }
  if (value && typeof value === "object" && !Array.isArray(value)) {
    const properties = rule.properties ?? {};
    for (const name of rule.required ?? []) if (!(name in value)) fail(`${label} violates schema required property '${name}'`);
    if (rule.additionalProperties === false) for (const name of Object.keys(value)) if (!(name in properties)) fail(`${label} violates schema additional property '${name}'`);
    for (const [name, property] of Object.entries(properties)) if (name in value) validateSchema(value[name], property, root, `${label}.${name}`);
  }
  if (rule.allOf && !rule.allOf.every((candidate) => schemaMatches(value, candidate, root))) fail(`${label} violates schema allOf`);
  if (rule.anyOf && !rule.anyOf.some((candidate) => schemaMatches(value, candidate, root))) fail(`${label} violates schema anyOf`);
  if (rule.not && schemaMatches(value, rule.not, root)) fail(`${label} violates schema not`);
  if (rule.oneOf && rule.oneOf.filter((candidate) => schemaMatches(value, candidate, root)).length !== 1) fail(`${label} violates schema oneOf`);
}
function schemaMatches(value, schema, root) {
  try { validateSchema(value, schema, root); return true; } catch { return false; }
}

export function validateInventory(inventory, schema = JSON.parse(readFileSync(inventorySchemaPath, "utf8"))) {
  validateSchema(inventory, schema, schema);
  if (!inventory || typeof inventory !== "object" || Array.isArray(inventory)) fail("inventory must be an object");
  if (inventory.version !== 1) fail("inventory version must be 1");
  if (JSON.stringify(inventory.cadences) !== JSON.stringify([...cadences])) fail("inventory cadences must use the closed supported values");
  if (JSON.stringify(inventory.evidenceClasses) !== JSON.stringify([...evidenceClasses])) fail("inventory evidenceClasses must use the closed supported values");
  const products = idMap(inventory.products, "products");
  const targets = idMap(inventory.targets, "targets");
  const dependencies = idMap(inventory.dependencies, "dependencies");
  const commands = idMap(inventory.commands, "commands");
  const healthChecks = idMap(inventory.healthChecks, "healthChecks");
  const useCases = idMap(inventory.useCases, "useCases");
  const scenarios = idMap(inventory.scenarios, "scenarios");
  const demos = idMap(inventory.demos, "demos");

  for (const command of commands.values()) {
    if (!commandPhases.has(command.phase)) fail(`command '${command.id}' has invalid phase`);
    safeRelativePath(command.reference, `command '${command.id}' reference`);
    text(command.expectedOutcome, `command '${command.id}' expectedOutcome`);
    if (command.command !== undefined) safeCommand(command.command, `command '${command.id}'`);
    else if (!commandStatuses.has(command.status)) fail(`command '${command.id}' requires a safe command or explicit supported status`);
    safeEnvironment(command.environment, `command '${command.id}' environment`);
  }
  for (const dependency of dependencies.values()) {
    if (!dependencyKinds.has(dependency.kind)) fail(`dependency '${dependency.id}' has invalid kind`);
    if (typeof dependency.mutable !== "boolean") fail(`dependency '${dependency.id}' mutable must be boolean`);
    text(dependency.owner, `dependency '${dependency.id}' owner`);
    text(dependency.note, `dependency '${dependency.id}' note`);
    references(dependency.preparationCommandIds, commands, `dependency '${dependency.id}' preparationCommandIds`);
    references(dependency.healthCheckIds, healthChecks, `dependency '${dependency.id}' healthCheckIds`);
    references(dependency.cleanupCommandIds, commands, `dependency '${dependency.id}' cleanupCommandIds`);
    for (const commandId of dependency.preparationCommandIds) if (commands.get(commandId).phase !== "preparation") fail(`dependency '${dependency.id}' preparation command '${commandId}' has the wrong phase`);
    for (const commandId of dependency.cleanupCommandIds) if (commands.get(commandId).phase !== "cleanup") fail(`dependency '${dependency.id}' cleanup command '${commandId}' has the wrong phase`);
    if (dependency.mutable === true && dependency.cleanupCommandIds.length === 0) fail(`mutable dependency '${dependency.id}' is missing cleanup`);
  }
  for (const check of healthChecks.values()) {
    if (!dependencies.has(check.dependencyId)) fail(`health check '${check.id}' references unknown dependency '${check.dependencyId}'`);
    if (!commands.has(check.commandId)) fail(`health check '${check.id}' references unknown command '${check.commandId}'`);
    text(check.expectedOutcome, `health check '${check.id}' expectedOutcome`);
    const command = commands.get(check.commandId);
    if (!new Set(["preparation", "status"]).has(command.phase)) fail(`health check '${check.id}' command must be preparation or status`);
  }
  for (const product of products.values()) {
    text(product.name, `product '${product.id}' name`);
    references(product.targetIds, targets, `product '${product.id}' targetIds`);
    references(product.demoIds, demos, `product '${product.id}' demoIds`);
  }
  for (const target of targets.values()) {
    text(target.platform, `target '${target.id}' platform`);
    if (!targetSupport.has(target.support)) fail(`target '${target.id}' has invalid support`);
    text(target.note, `target '${target.id}' note`);
  }
  for (const useCase of useCases.values()) {
    text(useCase.outcome, `use case '${useCase.id}' outcome`);
    for (const reference of array(useCase.sourceReferences, `use case '${useCase.id}' sourceReferences`)) safeRelativePath(reference, `use case '${useCase.id}' source reference`);
    references(useCase.scenarioIds, scenarios, `use case '${useCase.id}' scenarioIds`);
  }
  for (const scenario of scenarios.values()) {
    if (!products.has(scenario.productId)) fail(`scenario '${scenario.id}' references unknown product '${scenario.productId}'`);
    references(scenario.orderedUseCaseIds, useCases, `scenario '${scenario.id}' orderedUseCaseIds`);
    if (scenario.orderedUseCaseIds.length === 0) fail(`scenario '${scenario.id}' has no ordered use cases`);
    const targetPlans = idMap(array(scenario.targetPlans, `scenario '${scenario.id}' targetPlans`).map((plan) => ({ ...plan, id: plan.targetId })), `scenario '${scenario.id}' targetPlans`);
    if (targetPlans.size === 0) fail(`scenario '${scenario.id}' has no target plans`);
    if (!targetPlans.has(scenario.defaultTargetId)) fail(`scenario '${scenario.id}' default target is not planned`);
    for (const plan of targetPlans.values()) {
      if (!targets.has(plan.targetId)) fail(`scenario '${scenario.id}' target plan references unknown target '${plan.targetId}'`);
      if (!products.get(scenario.productId).targetIds.includes(plan.targetId)) fail(`scenario '${scenario.id}' target '${plan.targetId}' is not supported by product '${scenario.productId}'`);
      if (!targetStatuses.has(plan.status)) fail(`scenario '${scenario.id}' target '${plan.targetId}' has invalid status`);
      if (!evidenceClasses.has(plan.evidenceClass)) fail(`scenario '${scenario.id}' target '${plan.targetId}' has invalid evidence class`);
      references(plan.dependencyIds, dependencies, `scenario '${scenario.id}' target '${plan.targetId}' dependencyIds`);
      if (!plan.commandIds || typeof plan.commandIds !== "object" || Array.isArray(plan.commandIds)) fail(`scenario '${scenario.id}' target '${plan.targetId}' commandIds must be an object`);
      for (const phase of commandPhases) {
        const label = `scenario '${scenario.id}' target '${plan.targetId}' ${phase} commands`;
        references(plan.commandIds[phase], commands, label);
        for (const commandId of plan.commandIds[phase]) if (commands.get(commandId).phase !== phase) fail(`${label} references command '${commandId}' from phase '${commands.get(commandId).phase}'`);
      }
      references(plan.healthCheckIds, healthChecks, `scenario '${scenario.id}' target '${plan.targetId}' healthCheckIds`);
      for (const checkId of plan.healthCheckIds) if (!plan.dependencyIds.includes(healthChecks.get(checkId).dependencyId)) fail(`scenario '${scenario.id}' health check '${checkId}' is outside the target dependency boundary`);
      text(plan.note, `scenario '${scenario.id}' target '${plan.targetId}' note`);
    }
    if (targetPlans.get(scenario.defaultTargetId).status !== "supported") fail(`scenario '${scenario.id}' default target must be supported`);
    if (!evidenceClasses.has(scenario.evidenceClass)) fail(`scenario '${scenario.id}' has invalid evidence class`);
    if (!cadences.has(scenario.cadence)) fail(`scenario '${scenario.id}' has invalid cadence`);
    if (!scenario.testMapping || typeof scenario.testMapping !== "object") fail(`scenario '${scenario.id}' is missing a test mapping`);
    if (!["automated", "manual", "partial", "planned"].includes(scenario.testMapping.status)) fail(`scenario '${scenario.id}' has invalid test mapping status`);
    if (array(scenario.testMapping.automated, `scenario '${scenario.id}' automated mapping`).length === 0 && !text(scenario.testMapping.manual, `scenario '${scenario.id}' manual mapping`)) fail(`scenario '${scenario.id}' has no test mapping`);
    const manualSteps = array(scenario.manualSteps, `scenario '${scenario.id}' manualSteps`);
    if (manualSteps.length === 0) fail(`scenario '${scenario.id}' has no manual steps`);
    manualSteps.forEach((step, index) => text(step, `scenario '${scenario.id}' manual step ${index + 1}`));
    const expectedOutcomes = array(scenario.expectedOutcomes, `scenario '${scenario.id}' expectedOutcomes`);
    if (expectedOutcomes.length === 0) fail(`scenario '${scenario.id}' has no expected outcomes`);
    expectedOutcomes.forEach((outcome, index) => text(outcome, `scenario '${scenario.id}' expected outcome ${index + 1}`));
  }
  for (const demo of demos.values()) {
    if (!products.has(demo.productId)) fail(`demo '${demo.id}' references unknown product '${demo.productId}'`);
    references(demo.scenarioIds, scenarios, `demo '${demo.id}' scenarioIds`);
    references(demo.targetIds, targets, `demo '${demo.id}' targetIds`);
    for (const targetId of demo.targetIds) {
      if (!demo.scenarioIds.some((scenarioId) => scenarios.get(scenarioId).targetPlans.some((plan) => plan.targetId === targetId))) {
        fail(`demo '${demo.id}' target '${targetId}' is not planned by one of its scenarios`);
      }
    }
  }
  for (const [id, useCase] of useCases) for (const scenarioId of useCase.scenarioIds) {
    if (!scenarios.get(scenarioId).orderedUseCaseIds.includes(id)) fail(`use case '${id}' is not ordered by scenario '${scenarioId}'`);
  }
  for (const [id, scenario] of scenarios) for (const useCaseId of scenario.orderedUseCaseIds) {
    if (!useCases.get(useCaseId).scenarioIds.includes(id)) fail(`scenario '${id}' is not linked by use case '${useCaseId}'`);
  }
  for (const product of products.values()) for (const demoId of product.demoIds) {
    if (demos.get(demoId).productId !== product.id) fail(`product '${product.id}' links demo '${demoId}' owned by another product`);
  }
  for (const demo of demos.values()) {
    if (!products.get(demo.productId).demoIds.includes(demo.id)) fail(`demo '${demo.id}' is not linked by product '${demo.productId}'`);
    for (const scenarioId of demo.scenarioIds) if (scenarios.get(scenarioId).productId !== demo.productId) fail(`demo '${demo.id}' links scenario '${scenarioId}' owned by another product`);
  }
  return inventory;
}

export function loadInventory(inventoryPath = defaultInventoryPath) {
  let parsed; let schema;
  try { schema = JSON.parse(readFileSync(inventorySchemaPath, "utf8")); } catch (error) { fail(`cannot read inventory schema: ${error.message}`); }
  try { parsed = JSON.parse(readFileSync(inventoryPath, "utf8")); } catch (error) { fail(`cannot read inventory '${inventoryPath}': ${error.message}`); }
  return validateInventory(parsed, schema);
}
function commandDisplay(command) {
  const environment = Object.entries(command.environment ?? {}).map(([name, value]) => `${name}=${value}`).join(" ");
  return command.command ? `${environment ? `${environment} ` : ""}${command.command}` : `[${command.status}: ${command.expectedOutcome}]`;
}
export function renderList(inventory) {
  return inventory.scenarios.map((scenario) => {
    const targets = scenario.targetPlans.map((plan) => `${plan.targetId}:${plan.evidenceClass}`).join(",");
    return `${scenario.id}\t${scenario.evidenceClass}\t${scenario.cadence}\t${targets}\t${scenario.title}`;
  }).join("\n");
}
export function renderShow(inventory, id, kind = "scenario") {
  const collection = kind === "use-case" ? inventory.useCases : inventory.scenarios;
  const item = collection.find((entry) => entry.id === id);
  if (!item) fail(`unknown ${kind} '${id}'`);
  return `${kind}: ${item.id}\n${JSON.stringify(item, null, 2)}`;
}
export function renderPreparationBrief(inventory, id, requestedTargetId) {
  const scenario = inventory.scenarios.find((entry) => entry.id === id);
  if (!scenario) fail(`unknown scenario '${id}'`);
  const targetId = requestedTargetId ?? scenario.defaultTargetId;
  const plan = scenario.targetPlans.find((entry) => entry.targetId === targetId);
  if (!plan) fail(`scenario '${id}' does not support target '${targetId}'`);
  const byId = new Map(inventory.commands.map((command) => [command.id, command]));
  const lines = [`Scenario preparation brief: ${scenario.id}`, `Product: ${scenario.productId}`, `Target: ${targetId} (${plan.status}; ${plan.evidenceClass})`, `Scenario evidence: ${scenario.evidenceClass}; cadence: ${scenario.cadence}`, `Target boundary: ${plan.note}`, "This brief is validated inventory data, not a shell program. The invoking user and active AGENT.md define execution authority; preserve resource-hygiene and operator/receipt boundaries."];
  for (const phase of ["preparation", "build", "deploy", "run", "status", "cleanup"]) lines.push(`${phase}: ${(plan.commandIds[phase] ?? []).map((commandId) => commandDisplay(byId.get(commandId))).join(" | ") || "none"}`);
  lines.push(`Health checks: ${plan.healthCheckIds.map((checkId) => inventory.healthChecks.find((check) => check.id === checkId).expectedOutcome).join(" | ") || "none"}`);
  lines.push(`Manual acceptance: ${scenario.manualSteps.join(" ")}`);
  lines.push(`Expected outcomes: ${scenario.expectedOutcomes.join(" ")}`);
  return lines.join("\n");
}
function usage() { return "Usage: node scripts/demo-inventory.mjs check | list | show <scenario-id> | use-case [list|show <id>] | prepare <scenario-id> [target-id]"; }
function main(argv) {
  const args = [...argv]; let inventoryPath = defaultInventoryPath;
  if (args[0] === "--inventory") { inventoryPath = path.resolve(repoRoot, text(args[1], "inventory path")); args.splice(0, 2); }
  const inventory = loadInventory(inventoryPath); const [command, id] = args;
  if (command === "check") return "demo inventory: PASS";
  if (command === "list") return renderList(inventory);
  if (command === "show") return renderShow(inventory, id);
  if (command === "prepare") return renderPreparationBrief(inventory, id, args[2]);
  if (command === "use-case" && (!id || id === "list")) return inventory.useCases.map((item) => `${item.id}\t${item.outcome}`).join("\n");
  if (command === "use-case" && id === "show") return renderShow(inventory, args[2], "use-case");
  fail(usage());
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { console.log(main(process.argv.slice(2))); } catch (error) { console.error(`demo inventory: FAIL: ${error.message}`); process.exitCode = 1; }
}
