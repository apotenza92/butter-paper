import { configDefaults, defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  resolve: {
    alias: {
      '@butter-paper/core': fileURLToPath(new URL('./packages/core/src/index.ts', import.meta.url)),
      '@butter-paper/pdf/browser': fileURLToPath(new URL('./packages/pdf/src/browser.ts', import.meta.url)),
      '@butter-paper/pdf': fileURLToPath(new URL('./packages/pdf/src/index.ts', import.meta.url)),
    },
  },
  test: {
    include: ['packages/**/*.test.ts', 'apps/**/*.test.ts', 'tests/**/*.test.ts'],
    exclude: [...configDefaults.exclude, 'tests/e2e/**'],
    globals: true,
    environment: 'node',
    coverage: {
      reporter: ['text', 'html'],
    },
  },
});
