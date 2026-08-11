import { readFileSync } from 'node:fs';

describe('renderer filesystem boundary', () => {
  it('does not expose a generic renderer-to-main file write capability', () => {
    const ipc = readFileSync('apps/desktop/src/shared/ipc.ts', 'utf8');
    const protocol = readFileSync('apps/desktop/src/shared/protocol.ts', 'utf8');
    const preload = readFileSync('apps/desktop/src/preload/index.ts', 'utf8');
    const main = readFileSync('apps/desktop/src/main/window.ts', 'utf8');

    for (const source of [ipc, protocol, preload, main]) {
      expect(source).not.toMatch(/fileWrite|file:write|writeBinaryFile/);
    }

    expect(protocol).not.toMatch(/readonly files:\s*\{/);
    expect(protocol).toContain('readDocumentBytes(request: PdfDocumentAccessRequest): Promise<Uint8Array>');
    expect(preload).toContain('ipcRenderer.invoke(ipcChannels.fileRead, request)');
    expect(preload).not.toMatch(/readFile:\s*async\s*\(filePath/);
    expect(main).toContain('desktopPdfAccessRegistry.readDocumentBytes(event.sender.id, request.documentHandle)');
    expect(main).not.toMatch(/ipcChannels\.fileRead,[\s\S]{0,160}readBinaryFile\(filePath\)/);

    const saveRequest = protocol.slice(
      protocol.indexOf('export interface SaveDocumentRequest'),
      protocol.indexOf('export interface PageGeometryRequest'),
    );
    expect(saveRequest).toContain('readonly documentHandle: string');
    expect(saveRequest).toContain('readonly targetHandle: string');
    expect(saveRequest).not.toMatch(/sourcePath|targetPath|filePath/);
  });

  it('admits initial PDF paths only through main-owned source grants', () => {
    const main = readFileSync('apps/desktop/src/main/window.ts', 'utf8');
    const entry = readFileSync('apps/desktop/src/main/index.ts', 'utf8');
    const app = readFileSync('apps/desktop/src/renderer/src/app.tsx', 'utf8');

    expect(main).toContain('desktopPdfAccessRegistry.authorizeSource(event.sender.id, filePath)');
    expect(main).toContain('desktopPdfAccessRegistry.openAuthorizedSource(event.sender.id, filePath)');
    expect(main).toContain("name !== basename(name)");
    expect(entry).toContain('desktopPdfAccessRegistry.authorizeSource(window.webContents.id, filePath)');
    expect(app).toContain('window.butterPaper.test?.authorizePdfSource(filePath)');
    expect(app).not.toContain('openDocumentPath: loadDocumentFromPath');
  });

  it('authorizes operating-system-backed dropped PDFs before opening them', () => {
    const channels = readFileSync('apps/desktop/src/shared/ipc.ts', 'utf8');
    const protocol = readFileSync('apps/desktop/src/shared/protocol.ts', 'utf8');
    const preload = readFileSync('apps/desktop/src/preload/index.ts', 'utf8');
    const main = readFileSync('apps/desktop/src/main/window.ts', 'utf8');
    const app = readFileSync('apps/desktop/src/renderer/src/app.tsx', 'utf8');

    expect(channels).toContain("applicationAuthorizeDroppedPdf: 'application:authorize-dropped-pdf'");
    expect(protocol).toContain('authorizeDroppedPdf(file: File): Promise<string>');
    expect(preload).toContain('webUtils.getPathForFile(file)');
    expect(preload).toContain('ipcRenderer.invoke(ipcChannels.applicationAuthorizeDroppedPdf, filePath)');
    expect(main).toContain('desktopPdfAccessRegistry.authorizeSource(event.sender.id, filePath)');
    expect(app).toContain('openDocumentPaths(pdfPaths, { forceNewTabs: true })');
    expect(app).not.toMatch(/File & \{ path\?: string \}/);
  });

  it('locks packaged renderer navigation to local content and ships a restrictive CSP', () => {
    const main = readFileSync('apps/desktop/src/main/window.ts', 'utf8');
    const html = readFileSync('apps/desktop/src/renderer/index.html', 'utf8');

    expect(main).toContain("app.isPackaged || process.env.BP_DISABLE_RENDERER_DEV_SERVER === '1'");
    expect(main).toContain("window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))");
    expect(main).toMatch(/webContents\.on\('will-navigate',[\s\S]{0,100}event\.preventDefault\(\)/);
    expect(html).toContain('Content-Security-Policy');
    expect(html).toContain("object-src 'none'");
    expect(html).toContain("frame-src 'none'");
  });

  it('keeps Node crypto native in the Electron main bundle', () => {
    const config = readFileSync('apps/desktop/vite.main.config.ts', 'utf8');
    expect(config).toContain("'node:crypto'");
    expect(config).toContain('BP_SIGNATURE_RELAY_PRODUCTION_ORIGIN');
    const main = readFileSync('apps/desktop/src/main/window.ts', 'utf8');
    expect(main).toContain("!app.isPackaged && process.env.BP_SIGNATURE_RELAY_TEST_MODE === '1'");
  });
});
