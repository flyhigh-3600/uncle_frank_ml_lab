import { defineConfig } from "@playwright/test";

const port = Number(process.env.FRANK_E2E_PORT || 8789);

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  expect: { timeout: 10_000 },
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    browserName: "chromium",
    headless: true,
  },
  webServer: {
    command: `npm run build && cd .. && FRANK_ADDR=127.0.0.1:${port} cargo run --quiet -p frank-backend`,
    url: `http://127.0.0.1:${port}/api/health`,
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
    env: { FRANK_ADDR: `127.0.0.1:${port}` },
  },
});
