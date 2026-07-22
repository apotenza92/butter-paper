import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { createReadStream, readFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { defineConfig, type Plugin } from 'vite';

const pdfJsWasmFileNames = [
  'openjpeg.wasm',
  'openjpeg_nowasm_fallback.js',
  'qcms_bg.wasm',
] as const;
const rendererPort = Number.parseInt(process.env.BP_RENDERER_PORT ?? '5174', 10);

export default defineConfig({
  root: 'src/renderer',
  base: './',
  server: {
    host: '127.0.0.1',
    port: Number.isFinite(rendererPort) ? rendererPort : 5174,
    strictPort: true,
  },
  plugins: [react(), tailwindcss(), pdfJsWasmAssets()],
  resolve: {
    dedupe: ['react', 'react-dom'],
    alias: {
      '@butter-paper/core': resolve(__dirname, '../../packages/core/src/index.ts'),
      '@butter-paper/pdf/browser': resolve(__dirname, '../../packages/pdf/src/browser.ts'),
    },
  },
  build: {
    outDir: '../../.vite/renderer/main_window',
    emptyOutDir: false,
  },
});

function pdfJsWasmAssets(): Plugin {
  const sourceDir = resolve(__dirname, '../../node_modules/pdfjs-dist/wasm');
  const filePaths = pdfJsWasmFileNames.map((fileName) => join(sourceDir, fileName));

  return {
    name: 'butter-paper-pdfjs-wasm-assets',
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        if (!request.url?.startsWith('/assets/pdfjs-wasm/')) {
          next();
          return;
        }

        const fileName = basename(request.url);
        if (!pdfJsWasmFileNames.includes(fileName as (typeof pdfJsWasmFileNames)[number])) {
          next();
          return;
        }

        response.setHeader('Cache-Control', 'no-cache');
        response.setHeader('Content-Type', fileName.endsWith('.wasm') ? 'application/wasm' : 'text/javascript');
        createReadStream(join(sourceDir, fileName)).pipe(response);
      });
    },
    generateBundle() {
      for (const filePath of filePaths) {
        this.emitFile({
          type: 'asset',
          fileName: `assets/pdfjs-wasm/${basename(filePath)}`,
          source: readFileSync(filePath),
        });
      }
    },
  };
}
