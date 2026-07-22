export default {
  testDir: './tests/e2e',
  globalSetup: './tests/e2e/global-setup.mjs',
  outputDir: 'test-results/e2e',
  timeout: 90_000,
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: process.env.CI ? [['list'], ['html', { outputFolder: 'playwright-report' }]] : [['list']],
  expect: {
    timeout: 30_000,
  },
  use: {
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },
  projects: [
    {
      name: 'electron',
    },
  ],
};
