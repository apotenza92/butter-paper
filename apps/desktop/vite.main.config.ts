import { defineConfig } from 'vite';

const external = [
  'electron',
  'node:fs',
  'node:fs/promises',
  'node:module',
  'node:path',
  'node:url',
  '@butter-paper/core',
  '@butter-paper/pdf',
  '@napi-rs/canvas',
  'pdf-lib',
  'pdfjs-dist',
  /^pdfjs-dist\//,
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
