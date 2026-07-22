import { defineConfig } from 'vite';

const external = [
  'electron',
  'node:fs/promises',
  'node:module',
  'node:path',
  'node:url',
  '@butter-paper/core',
  '@butter-paper/pdf',
];

export default defineConfig({
  build: {
    outDir: '.vite/build',
    emptyOutDir: false,
    lib: {
      entry: 'src/preload/index.ts',
      formats: ['cjs'],
      fileName: () => 'preload.cjs',
    },
    rollupOptions: {
      external,
    },
  },
});
