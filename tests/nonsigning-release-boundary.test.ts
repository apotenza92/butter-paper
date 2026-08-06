import { existsSync, readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';

const forbiddenDomainPaths = [
  'native/pdf-signature-core',
  'native/pdfium-render-core',
  'packages/core/src/signatures.ts',
  'packages/pdf/src/signatureSafety.ts',
  'apps/desktop/src/main/pdfSignatureCore.ts',
  'apps/desktop/src/main/signatureDocumentRegistry.ts',
  'apps/desktop/src/main/signatureTrustStore.ts',
  'apps/desktop/src/renderer/src/components/SignatureDetailsPanel.tsx',
  'apps/desktop/src/renderer/src/components/SignatureTrustDialog.tsx',
  'apps/desktop/src/renderer/src/components/ProtectedDocumentBanner.tsx',
];

describe('0.0.18 non-signing release boundary', () => {
  it('contains neither native PDF signing nor PDFium render backends', () => {
    for (const path of forbiddenDomainPaths) {
      expect(existsSync(path), `${path} must remain absent`).toBe(false);
    }
  });

  it('exposes no PDF signature, trust, protection, or PDFium renderer contract', () => {
    const files = [
      'apps/desktop/src/shared/ipc.ts',
      'apps/desktop/src/shared/protocol.ts',
      'apps/desktop/src/preload/index.ts',
      'apps/desktop/src/main/window.ts',
      'apps/desktop/src/main/pdfSession.ts',
      'apps/desktop/src/renderer/src/app.tsx',
      'apps/desktop/src/renderer/src/state/viewerStore.ts',
      'packages/core/src/index.ts',
      'packages/pdf/src/index.ts',
    ];
    const source = files.map((path) => readFileSync(path, 'utf8')).join('\n');
    expect(source).not.toMatch(/pdfSignature|signatureDocument|signatureTrust|signatureProtection/i);
    expect(source).not.toMatch(/createUnsignedCopy|Save Original Copy|Offline Certificate Trust/i);
    expect(source).not.toMatch(/BP_DESKTOP_RENDER_BACKEND|pdfium|renderCore/i);
  });

  it('keeps forbidden domain paths out of the tracked candidate tree', () => {
    const trackedPaths = execFileSync('git', ['ls-files'], { encoding: 'utf8' })
      .split('\n')
      .filter((path) => path && existsSync(path))
      .join('\n');
    expect(trackedPaths).not.toMatch(/(?:^|\/)native\/pdf-signature-core\//m);
    expect(trackedPaths).not.toMatch(/(?:^|\/)native\/pdfium-render-core\//m);
    expect(trackedPaths).not.toMatch(/(?:^|\/)(?:pdfSignature|signatureTrust|signatureDocument|signatureSafety)/im);
  });
});
