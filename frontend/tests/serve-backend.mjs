// Test-only process harness: mock mail stays on disk in an isolated temporary directory.
import { spawn } from "node:child_process";
import { mkdtempSync, createWriteStream, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { randomBytes } from "node:crypto";
const dir = mkdtempSync(join(tmpdir(), "bluestaq-e2e-"));
writeFileSync(
  process.env.E2E_INBOX_FILE || ".svelte-kit/e2e-inbox-path.txt",
  join(dir, "mail.log"),
);
const output = createWriteStream(join(dir, "mail.log"), { mode: 0o600 });
const child = spawn(
  "cargo",
  ["run", "--manifest-path", "../backend/Cargo.toml"],
  {
    cwd: process.cwd(),
    env: {
      ...process.env,
      DATABASE_URL: join(dir, "notes.sqlite"),
      BIND: `127.0.0.1:${process.env.E2E_API_PORT || "18080"}`,
      SECURE_COOKIES: "false",
      TOKEN_SIGNING_KEY_B64: randomBytes(48).toString("base64"),
      PASSWORD_PEPPER_B64: randomBytes(48).toString("base64"),
    },
    stdio: ["ignore", "pipe", "pipe"],
  },
);
child.stdout.pipe(output);
child.stderr.pipe(process.stderr);
let shuttingDown = false;
function stop() {
  if (shuttingDown) return;
  shuttingDown = true;
  child.kill("SIGTERM");
}
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
child.on("exit", (code) => {
  output.end();
  rmSync(dir, { recursive: true, force: true });
  process.exit(code ?? 0);
});
