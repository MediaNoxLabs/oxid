// SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmod, cp, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

async function executable(file, source) {
  await writeFile(file, source, { mode: 0o700 });
  await chmod(file, 0o700);
}

test("Tailnet faucet owns one route and restores unrelated Serve state", async (context) => {
  const fixture = await mkdtemp(path.join(os.tmpdir(), "oxid-faucet-tailnet-"));
  context.after(() => rm(fixture, { recursive: true, force: true }));
  const fixtureScripts = path.join(fixture, "scripts");
  const fixtureScriptLib = path.join(fixtureScripts, "lib");
  const fakeBin = path.join(fixture, "bin");
  await mkdir(fixtureScripts);
  await mkdir(fixtureScriptLib);
  await mkdir(fakeBin);
  await cp(path.join(root, "scripts/standalone-faucet-tailnet.sh"), path.join(fixtureScripts, "standalone-faucet-tailnet.sh"));
  await chmod(path.join(fixtureScripts, "standalone-faucet-tailnet.sh"), 0o700);
  await cp(path.join(root, "scripts/lib/spawn-detached.mjs"), path.join(fixtureScriptLib, "spawn-detached.mjs"));
  await executable(path.join(fixtureScripts, "standalone-status.sh"), "#!/bin/sh\nexit 0\n");

  const baseline = { TCP: { "2222": { TCPForward: "127.0.0.1:22" } }, Web: {} };
  const serveState = path.join(fixture, "serve.json");
  await writeFile(serveState, JSON.stringify(baseline));
  const faucetBinary = path.join(fixture, "fake-faucet.mjs");
  await executable(faucetBinary, "#!/usr/bin/env node\nprocess.on('SIGTERM',()=>process.exit(0));setInterval(()=>{},1000);\n");
  await executable(path.join(fakeBin, "cargo"), `#!/usr/bin/env node
console.log(JSON.stringify({reason:"compiler-artifact",target:{name:"oxid-standalone-faucet-http"},executable:${JSON.stringify(faucetBinary)}}));
`);
  await executable(path.join(fakeBin, "curl"), `#!/usr/bin/env node
const url=process.argv.at(-1);if(url.endsWith('/fund'))console.log('{"ok":true,"result":{"receipt":{"amount":{"atomicUnits":"50000000000"}}}}');
`);
  await executable(path.join(fakeBin, "qrencode"), `#!/usr/bin/env node
import{writeFileSync}from'node:fs';const output=process.argv.find(v=>v.startsWith('--output=')).slice(9);writeFileSync(output,'<svg/>');
`);
  await executable(path.join(fakeBin, "tailscale"), `#!/usr/bin/env node
import{readFileSync,writeFileSync}from'node:fs';
const args=process.argv.slice(2),file=process.env.FAKE_TAILSCALE_STATE;
const state=()=>JSON.parse(readFileSync(file,'utf8'));
if(args[0]==='status'){console.log(JSON.stringify({BackendState:'Running',Self:{DNSName:'fixture.example.ts.net.'}}));process.exit(0)}
if(args[0]==='serve'&&args[1]==='status'){console.log(JSON.stringify(state()));process.exit(0)}
if(args[0]==='serve'){
  const port=args.find(v=>v.startsWith('--https=')).slice(8),key='fixture.example.ts.net:'+port;
  if(args.at(-1)==='off'){writeFileSync(file,JSON.stringify(${JSON.stringify(baseline)}));process.exit(0)}
  const next=state();next.Web[key]??={Handlers:{}};
  const setPath=args.find(v=>v.startsWith('--set-path='));
  next.Web[key].Handlers[setPath?setPath.slice(11):'/']={Proxy:args.at(-1)};
  writeFileSync(file,JSON.stringify(next));process.exit(0)
}
process.exit(2);
`);

  const env = {
    ...process.env,
    PATH: `${fakeBin}:${process.env.PATH}`,
    FAKE_TAILSCALE_STATE: serveState,
    OXID_ENABLE_OWNER_TAILNET_FAUCET_ACCEPTANCE: "1",
    OXID_FAUCET_RECIPIENT_ADDRESS: "mn_addr_undeployed1fixture",
  };
  const lifecycle = path.join(fixtureScripts, "standalone-faucet-tailnet.sh");
  for (const mode of ["start", "status", "accept", "stop"]) {
    const result = spawnSync(lifecycle, [mode], { env, encoding: "utf8", timeout: 10_000 });
    assert.equal(result.status, 0, `${mode}: ${result.stderr}`);
  }
  assert.deepEqual(JSON.parse(await readFile(serveState, "utf8")), baseline);
  await assert.rejects(readFile(path.join(fixture, "target/standalone-faucet-tailnet/receipt.json")));
});

test("Tailnet source contract forbids broad Serve or state deletion", async () => {
  const script = await readFile(path.join(root, "scripts/standalone-faucet-tailnet.sh"), "utf8");
  assert.match(script, /OXID_ENABLE_OWNER_TAILNET_FAUCET_ACCEPTANCE/u);
  assert.doesNotMatch(script, /tailscale serve reset/u);
  assert.doesNotMatch(script, /--set-path/u);
  assert.doesNotMatch(script, /\bfunnel\b/u);
  assert.doesNotMatch(script, /rm -rf/u);
  assert.match(script, /spawn-detached\.mjs/u);
  assert.match(script, /env -i PATH=/u);
  assert.match(script, /process_has_exited/u);
  assert.match(script, /\[\[ "\$state" == Z\* \]\]/u);

  const launcher = await readFile(path.join(root, "scripts/lib/spawn-detached.mjs"), "utf8");
  assert.match(launcher, /detached: true/u);
  assert.match(launcher, /stdio: \["ignore", log, log\]/u);

  const justfile = await readFile(path.join(root, "Justfile"), "utf8");
  assert.match(justfile, /^standalone-faucet-tailnet-lifecycle-test:/mu);
});

test("round-trip cleanup attempts both independently owned layers", async (context) => {
  const fixture = await mkdtemp(path.join(os.tmpdir(), "oxid-tailnet-cleanup-"));
  context.after(() => rm(fixture, { recursive: true, force: true }));
  const scripts = path.join(fixture, "scripts");
  const calls = path.join(fixture, "calls");
  await mkdir(scripts);
  await mkdir(path.join(fixture, "target/standalone-faucet-tailnet"), { recursive: true });
  await mkdir(path.join(fixture, "target/standalone-tailnet-routes"), { recursive: true });
  await cp(path.join(root, "scripts/standalone-tailnet-round-trip.sh"), path.join(scripts, "standalone-tailnet-round-trip.sh"));
  await chmod(path.join(scripts, "standalone-tailnet-round-trip.sh"), 0o700);
  await executable(path.join(scripts, "standalone-faucet-tailnet.sh"), "#!/bin/sh\nprintf 'faucet\\n' >>\"$CALLS\"\nexit 1\n");
  await executable(path.join(scripts, "standalone-tailnet-routes.sh"), "#!/bin/sh\nprintf 'routes\\n' >>\"$CALLS\"\nexit 0\n");

  const result = spawnSync(path.join(scripts, "standalone-tailnet-round-trip.sh"), ["stop"], {
    env: { ...process.env, CALLS: calls },
    encoding: "utf8",
  });
  assert.equal(result.status, 1);
  assert.deepEqual((await readFile(calls, "utf8")).trim().split("\n"), ["faucet", "routes"]);
});

test("service-route cleanup resumes after a later route removal fails", async (context) => {
  const fixture = await mkdtemp(path.join(os.tmpdir(), "oxid-tailnet-routes-"));
  context.after(() => rm(fixture, { recursive: true, force: true }));
  const scripts = path.join(fixture, "scripts");
  const fakeBin = path.join(fixture, "bin");
  const serveState = path.join(fixture, "serve.json");
  const failureMarker = path.join(fixture, "failed-once");
  const baseline = { TCP: { "2222": { TCPForward: "127.0.0.1:22" } }, Web: {} };
  await mkdir(scripts);
  await mkdir(fakeBin);
  await writeFile(serveState, JSON.stringify(baseline));
  await cp(path.join(root, "scripts/standalone-tailnet-routes.sh"), path.join(scripts, "standalone-tailnet-routes.sh"));
  await chmod(path.join(scripts, "standalone-tailnet-routes.sh"), 0o700);
  await executable(path.join(scripts, "standalone-status.sh"), "#!/bin/sh\nexit 0\n");
  await executable(path.join(fakeBin, "curl"), "#!/bin/sh\nexit 0\n");
  await executable(path.join(fakeBin, "tailscale"), `#!/usr/bin/env node
import{existsSync,readFileSync,writeFileSync}from'node:fs';
const args=process.argv.slice(2),file=process.env.FAKE_TAILSCALE_STATE;
const state=()=>JSON.parse(readFileSync(file,'utf8'));
if(args[0]==='status'){console.log(JSON.stringify({BackendState:'Running',Self:{DNSName:'fixture.example.ts.net.'}}));process.exit(0)}
if(args[0]==='serve'&&args[1]==='status'){console.log(JSON.stringify(state()));process.exit(0)}
if(args[0]==='serve'){
  const port=args.find(v=>v.startsWith('--https=')).slice(8),key='fixture.example.ts.net:'+port;
  if(args.at(-1)==='off'){
    if(port===process.env.FAKE_FAIL_OFF_PORT&&!existsSync(process.env.FAKE_FAILURE_MARKER)){writeFileSync(process.env.FAKE_FAILURE_MARKER,'failed');process.exit(1)}
    const next=state();delete next.TCP[port];delete next.Web[key];writeFileSync(file,JSON.stringify(next));process.exit(0)
  }
  const next=state();next.TCP[port]={HTTPS:true};next.Web[key]={Handlers:{'/':{Proxy:args.at(-1)}}};writeFileSync(file,JSON.stringify(next));process.exit(0)
}
process.exit(2);
`);
  const env = {
    ...process.env,
    PATH: `${fakeBin}:${process.env.PATH}`,
    FAKE_TAILSCALE_STATE: serveState,
    FAKE_FAIL_OFF_PORT: "12001",
    FAKE_FAILURE_MARKER: failureMarker,
  };
  const lifecycle = path.join(scripts, "standalone-tailnet-routes.sh");
  const start = spawnSync(lifecycle, ["start"], { env, encoding: "utf8" });
  assert.equal(start.status, 0, start.stderr);
  const firstStop = spawnSync(lifecycle, ["stop"], { env, encoding: "utf8" });
  assert.equal(firstStop.status, 1);
  const partial = JSON.parse(await readFile(path.join(fixture, "target/standalone-tailnet-routes/receipt.json"), "utf8"));
  assert.equal(partial.configured, 2, firstStop.stderr);
  const retry = spawnSync(lifecycle, ["stop"], { env, encoding: "utf8" });
  assert.equal(retry.status, 0, retry.stderr);
  assert.deepEqual(JSON.parse(await readFile(serveState, "utf8")), baseline);
  await assert.rejects(readFile(path.join(fixture, "target/standalone-tailnet-routes/receipt.json")));
});

test("mobile Tailnet route preparation is receipt-scoped and has no committed endpoint", async () => {
  const [routes, iosRunner, androidRunner, justfile] = await Promise.all([
    readFile(path.join(root, "scripts/standalone-tailnet-routes.sh"), "utf8"),
    readFile(path.join(root, "scripts/run-ios-simulator.sh"), "utf8"),
    readFile(path.join(root, "scripts/run-android-tailnet.sh"), "utf8"),
    readFile(path.join(root, "Justfile"), "utf8"),
  ]);
  assert.match(routes, /oxid-standalone-tailnet-routes-v1/u);
  assert.match(routes, /tailscale status --json/u);
  assert.match(routes, /tailscale serve status --json/u);
  assert.match(routes, /seq 12000 12999/u);
  assert.match(routes, /http:\/\/127\.0\.0\.1:8088/u);
  assert.match(routes, /http:\/\/127\.0\.0\.1:9944/u);
  assert.match(routes, /http:\/\/127\.0\.0\.1:6300/u);
  assert.doesNotMatch(routes, /tailscale serve reset/u);
  assert.doesNotMatch(routes, /\bfunnel\b/u);
  assert.doesNotMatch(routes, /rm -rf/u);
  assert.match(routes, /oxid-standalone-faucet-tailnet-v1/u);
  assert.match(routes, /\.baseline == \$baseline and \.active == \$active/u);
  assert.match(iosRunner, /OXID_STANDALONE_NETWORK_PROFILE=tailnet/u);
  assert.match(iosRunner, /standalone-tailnet-routes\.sh" status/u);
  assert.match(iosRunner, /standalone-tailnet/u);
  assert.match(iosRunner, /OXID_BUILD_MIDNIGHT_INDEXER_WS_URL/u);
  assert.match(iosRunner, /tailnet_artifact_binding/u);
  assert.match(iosRunner, /tailnet=\$tailnet_artifact_binding/u);
  assert.match(androidRunner, /standalone-tailnet-routes\/receipt\.json/u);
  assert.match(androidRunner, /standalone-tailnet-routes\.sh" status/u);
  assert.match(androidRunner, /OXID_BUILD_MIDNIGHT_INDEXER_WS_URL/u);
  assert.match(androidRunner, /OXID_MOBILE_PORTAL_PROFILE:-unavailable/u);
  assert.match(androidRunner, /\[ "\$proof_port" -eq 443 \]/u);
  assert.match(androidRunner, /OXID_BUILD_MIDNIGHT_PROOF_SERVER_URL="https:\/\/\$tailnet_dns_name"/u);
  const androidBuild = await readFile(path.join(root, "scripts/run-android-emulator.sh"), "utf8");
  assert.match(androidBuild, /tailnet_artifact_binding/u);
  assert.match(androidBuild, /tailnet=\$tailnet_artifact_binding/u);
  assert.match(justfile, /^standalone-tailnet-round-trip-start:/mu);
  assert.match(justfile, /^ios-standalone-tailnet:/mu);
});

test("the setup QR opens the private HTTPS funding page in an ordinary phone camera", async () => {
  const lifecycle = await readFile(path.join(root, "scripts/standalone-faucet-tailnet.sh"), "utf8");
  assert.match(lifecycle, /payload="https:\/\/\$dns:\$port\/"/u);
  assert.doesNotMatch(lifecycle, /payload="oxid-faucet:/u);
});
