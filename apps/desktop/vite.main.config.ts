import { defineConfig } from 'vite';

const external = [
  'electron',
  'node:crypto',
  'node:fs',
  'node:fs/promises',
  'node:http',
  'node:module',
  'node:os',
  'node:path',
  'node:url',
  '@butter-paper/core',
  '@butter-paper/pdf',
  '@butter-paper/pdf/blank',
  '@napi-rs/canvas',
  'pdf-lib',
  'pdfjs-dist',
  'tuf-js',
  /^pdfjs-dist\//,
  /^tuf-js\//,
];

export default defineConfig({
  define: {
    BP_SIGNATURE_RELAY_PRODUCTION_ORIGIN: JSON.stringify(
      process.env.BP_SIGNATURE_RELAY_PRODUCTION_ORIGIN?.trim() ?? '',
    ),
  },
  build: {
    outDir: '.vite/build',
    emptyOutDir: false,
    lib: {
      entry: 'src/main/index.ts',
      formats: ['es'],
      fileName: () => 'main.js',
    },
    rollupOptions: {
      external,
    },
  },
});
