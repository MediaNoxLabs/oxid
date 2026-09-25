import fs from "node:fs";
process.on("SIGTERM", () => fs.writeFileSync(process.env.OXID_FAKE_EMULATOR_TERM, "TERM\n"));
setInterval(() => {}, 1000);
