import { existsSync, readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';

const forbiddenDomainPaths = [
  'native/pdfium-render-core',
  'apps/desktop/src/renderer/src/components/SignatureDetailsPanel.tsx',
  'apps/desktop/src/renderer/src/components/SignatureTrustDialog.tsx',
  'apps/desktop/src/renderer/src/components/ProtectedDocumentBanner.tsx',
];

describe('PDF signing integration boundary', () => {
  it('contains no PDFium render backend', () => {
    for (const path of forbiddenDomainPaths) {
      expect(existsSync(path), `${path} must remain absent`).toBe(false);
    }
  });

  it('keeps the renderer session free of privileged signing controls', () => {
    const files = [
      'apps/desktop/src/renderer/src/app.tsx',
      'apps/desktop/src/renderer/src/state/viewerStore.ts',
    ];
    const source = files.map((path) => readFileSync(path, 'utf8')).join('\n');
    expect(source).not.toMatch(/ipcRenderer|electron|node:(?:fs|child_process|path)|process\./i);
    expect(source).not.toMatch(/signing:(?:approve|choose-identity)|privateKey|password|pkcs12|pfx/i);
    expect(source).not.toMatch(/BP_DESKTOP_RENDER_BACKEND|pdfium|renderCore/i);
  });

  it('keeps forbidden domain paths out of the tracked candidate tree', () => {
    const trackedPaths = execFileSync('git', ['ls-files'], { encoding: 'utf8' })
      .split('\n')
      .filter((path) => path && existsSync(path))
      .join('\n');
    expect(trackedPaths).not.toMatch(/(?:^|\/)native\/pdfium-render-core\//m);
  });
});
