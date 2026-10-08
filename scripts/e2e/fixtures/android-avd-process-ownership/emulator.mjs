import fs from "node:fs";
process.on("SIGTERM", () => fs.writeFileSync(process.env.OXID_FAKE_EMULATOR_TERM, "TERM\n"));
if (process.env.OXID_FAKE_EMULATOR_READY) {
  fs.writeFileSync(process.env.OXID_FAKE_EMULATOR_READY, "ready\n");
}
setInterval(() => {}, 1000);
