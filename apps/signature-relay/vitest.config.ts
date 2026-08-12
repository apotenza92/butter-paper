import { cloudflareTest } from '@cloudflare/vitest-pool-workers';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [
    cloudflareTest({
      main: './src/index.ts',
      miniflare: {
        compatibilityDate: '2026-08-11',
        bindings: {
          RELAY_ENABLED: 'true',
          RATE_LIMIT_TEST_BYPASS: 'true',
        },
        durableObjects: {
          SIGNATURE_SESSIONS: {
            className: 'SignatureSession',
            useSQLite: true,
          },
        },
      },
    }),
  ],
  test: {
    include: ['test/**/*.spec.ts'],
  },
});
