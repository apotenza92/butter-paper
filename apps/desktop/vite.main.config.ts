import { defineConfig } from 'vite';

const external = [
  'electron',
  'node:child_process',
  'node:crypto',
  'node:fs',
  'node:fs/promises',
  'node:http',
  'node:module',
  'node:path',
  'node:url',
  '@butter-paper/core',
  '@butter-paper/pdf',
  '@napi-rs/canvas',
  'pdf-lib',
  'pdfjs-dist',
  'tuf-js',
  /^pdfjs-dist\//,
  /^tuf-js\//,
];

export default defineConfig({
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
