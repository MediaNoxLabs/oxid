// SPDX-License-Identifier: Apache-2.0

import process from "node:process";

import { supervise } from "../../scripts/e2e/ios-xcode-supervisor.mjs";

supervise(process.argv.slice(2), { contenders: [] }).then(
  (code) => { process.exitCode = code; },
  (error) => {
    process.stderr.write(`ios-xcode-supervisor-test-driver: ${error.message}\n`);
    process.exitCode = 1;
  },
);
