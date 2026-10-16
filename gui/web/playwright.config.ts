import { defineConfig, devices } from '@playwright/test';

// The global setup starts `nk serve --demo` and a Vite dev server proxying
// /v1 to it, and exposes NK_GUI_URL / NK_TOKEN to the tests.
export default defineConfig({
  testDir: 'tests/e2e',
  globalSetup: './tests/e2e/global-setup.ts',
  timeout: 30_000,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: {
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
