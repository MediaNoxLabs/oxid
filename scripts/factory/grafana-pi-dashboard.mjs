#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const PLUGIN = "yesoreyeram-infinity-datasource";
const PLUGIN_VERSION = "4.1.0";
const BREW_PREFIX = process.env.HOMEBREW_PREFIX || "/opt/homebrew";
const GRAFANA_HOME = path.join(BREW_PREFIX, "opt", "grafana", "share", "grafana");
const CONFIG_ROOT = path.join(BREW_PREFIX, "etc", "grafana");
const DATA_ROOT = path.join(BREW_PREFIX, "var", "lib", "grafana");
const PLUGIN_ROOT = path.join(DATA_ROOT, "plugins", PLUGIN);
const PROVISIONING_ROOT = path.join(CONFIG_ROOT, "provisioning");
const DASHBOARD_ROOT = path.join(DATA_ROOT, "dashboards", "oxid");
const DATASOURCE_FILE = path.join(PROVISIONING_ROOT, "datasources", "oxid-pi-factory.yaml");
const PROVIDER_FILE = path.join(PROVISIONING_ROOT, "dashboards", "oxid-pi-factory.yaml");
const DASHBOARD_FILE = path.join(DASHBOARD_ROOT, "oxid-pi-factory.json");
const INI_FILE = path.join(CONFIG_ROOT, "grafana.ini");
const INI_MARKER = "# Managed by Oxid issue #923: local Pi factory dashboard";
const STATE_ROOT = path.join(os.homedir(), ".local", "state", "oxid", "pi-observability");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited ${result.status}: ${(result.stderr || result.stdout).trim()}`);
  return typeof result.stdout === "string" ? result.stdout.trim() : "";
}

async function pluginVersion() {
  if (!existsSync(path.join(PLUGIN_ROOT, "plugin.json"))) return null;
  return JSON.parse(await readFile(path.join(PLUGIN_ROOT, "plugin.json"), "utf8")).info?.version ?? null;
}

async function configureProvisioningPath() {
  const ini = await readFile(INI_FILE, "utf8");
  const target = `provisioning = ${PROVISIONING_ROOT}`;
  if (ini.includes(`${INI_MARKER}\n${target}`)) return;
  const active = ini.match(/^provisioning\s*=\s*(.+)$/mu);
  if (active) throw new Error(`refusing to replace an existing Grafana provisioning path: ${active[1].trim()}`);
  const defaultLine = ";provisioning = conf/provisioning";
  if (!ini.includes(defaultLine)) throw new Error("Grafana provisioning default was not found; configure it manually");
  await mkdir(STATE_ROOT, { recursive: true });
  await writeFile(path.join(STATE_ROOT, "grafana.ini.before-oxid-provisioning"), ini, { mode: 0o600 });
  await writeFile(INI_FILE, ini.replace(defaultLine, `${INI_MARKER}\n${target}`));
}

async function install() {
  if (!existsSync(INI_FILE) || !existsSync(GRAFANA_HOME)) {
    throw new Error("Homebrew Grafana is not installed at the expected paths");
  }
  if (await pluginVersion() !== PLUGIN_VERSION) {
    run("grafana", ["cli", "--homepath", GRAFANA_HOME, "--pluginsDir", path.join(DATA_ROOT, "plugins"),
      "plugins", "install", PLUGIN, PLUGIN_VERSION], { stdio: "inherit" });
  }
  await configureProvisioningPath();
  await mkdir(path.dirname(DATASOURCE_FILE), { recursive: true });
  await mkdir(path.dirname(PROVIDER_FILE), { recursive: true });
  for (const directory of ["alerting", "plugins", "notifiers", "access-control"]) {
    await mkdir(path.join(PROVISIONING_ROOT, directory), { recursive: true });
  }
  await mkdir(DASHBOARD_ROOT, { recursive: true });
  await writeFile(DATASOURCE_FILE, `# Managed by Oxid issue #923\napiVersion: 1\n\ndatasources:\n  - name: Oxid Pi Agento11y\n    uid: oxid-pi-agento11y\n    type: yesoreyeram-infinity-datasource\n    access: proxy\n    editable: false\n    jsonData:\n      allowedHosts:\n        - http://127.0.0.1:8765\n      timeoutInSeconds: 5\n`);
  await writeFile(PROVIDER_FILE, `# Managed by Oxid issue #923\napiVersion: 1\n\nproviders:\n  - name: Oxid Pi Factory\n    folder: Oxid Factory\n    type: file\n    disableDeletion: false\n    allowUiUpdates: false\n    updateIntervalSeconds: 10\n    options:\n      path: ${DASHBOARD_ROOT}\n`);
  await copyFile(path.join(REPO_ROOT, "docs", "factory", "grafana", "oxid-pi-factory.json"), DASHBOARD_FILE);
  run("brew", ["services", "restart", "grafana"], { stdio: "inherit" });
  let healthy = false;
  for (let attempt = 0; attempt < 60 && !healthy; attempt += 1) {
    try {
      const response = await fetch("http://127.0.0.1:3000/api/health", { signal: AbortSignal.timeout(1000) });
      healthy = response.ok;
    } catch { /* Grafana is still starting. */ }
    if (!healthy) await new Promise((resolve) => setTimeout(resolve, 500));
  }
  if (!healthy) throw new Error("Grafana did not become healthy after restart");
  process.stdout.write("Grafana dashboard installed: http://127.0.0.1:3000/d/oxid-pi-factory\n");
}

async function status() {
  let grafanaHealthy = false;
  try {
    const response = await fetch("http://127.0.0.1:3000/api/health", { signal: AbortSignal.timeout(1500) });
    grafanaHealthy = response.ok;
  } catch { /* Optional local service. */ }
  process.stdout.write(`${JSON.stringify({
    grafanaHealthy,
    plugin: { expected: PLUGIN_VERSION, installed: await pluginVersion() },
    datasourceProvisioned: existsSync(DATASOURCE_FILE),
    dashboardProvisioned: existsSync(DASHBOARD_FILE),
    url: "http://127.0.0.1:3000/d/oxid-pi-factory",
  }, null, 2)}\n`);
}

async function remove() {
  for (const file of [DATASOURCE_FILE, PROVIDER_FILE, DASHBOARD_FILE]) await rm(file, { force: true });
  const ini = await readFile(INI_FILE, "utf8");
  const managed = `${INI_MARKER}\nprovisioning = ${PROVISIONING_ROOT}`;
  if (ini.includes(managed)) await writeFile(INI_FILE, ini.replace(managed, ";provisioning = conf/provisioning"));
  run("brew", ["services", "restart", "grafana"], { stdio: "inherit" });
  process.stdout.write("Oxid Grafana provisioning removed; the shared Infinity plugin was retained.\n");
}

const command = process.argv[2];
try {
  if (command === "install" && process.argv.length === 3) await install();
  else if (command === "status" && process.argv.length === 3) await status();
  else if (command === "remove" && process.argv.length === 3) await remove();
  else throw new Error("Usage: node scripts/factory/grafana-pi-dashboard.mjs <install|status|remove>");
} catch (error) {
  process.stderr.write(`[grafana-pi-dashboard] ${error.message}\n`);
  process.exitCode = 1;
}
