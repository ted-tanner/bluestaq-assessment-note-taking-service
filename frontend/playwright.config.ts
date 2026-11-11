import { defineConfig, devices } from "@playwright/test";
import { mkdirSync } from "node:fs";
mkdirSync(process.env.E2E_SCREENSHOT_DIR || "test-results", {
  recursive: true,
});
const webPort = process.env.E2E_WEB_PORT || "15173";
const apiPort = process.env.E2E_API_PORT || "18080";
export default defineConfig({
  testDir: "./tests",
  testMatch: "**/*.spec.ts",
  fullyParallel: false,
  workers: 1,
  timeout: 45000,
  use: { baseURL: `http://127.0.0.1:${webPort}`, trace: "retain-on-failure" },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: [
    {
      command: "node tests/serve-backend.mjs",
      url: `http://127.0.0.1:${apiPort}/api/health`,
      timeout: 120000,
      reuseExistingServer: false,
    },
    {
      command: `pnpm dev --host 127.0.0.1 --port ${webPort}`,
      url: `http://127.0.0.1:${webPort}`,
      env: {
        API_PROXY_TARGET: `http://127.0.0.1:${apiPort}`,
        E2E_DISABLE_HMR: "true",
      },
      timeout: 30000,
      reuseExistingServer: false,
    },
  ],
});
