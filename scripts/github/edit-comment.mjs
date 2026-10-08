#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
import { runDevLoopsPackageScript } from "../lib/dev-loop-package-script.mjs";
process.exitCode = await runDevLoopsPackageScript("scripts/github/edit-comment.mjs");
