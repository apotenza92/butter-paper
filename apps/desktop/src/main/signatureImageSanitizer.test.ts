import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

describe('phone signature image sanitizer boundary', () => {
  it('uses a disposable sandbox without preload, permissions, or network access', () => {
    const source = readFileSync(new URL('./signatureImageSanitizer.ts', import.meta.url), 'utf8');
    expect(source).toContain('sandbox: true');
    expect(source).toContain('contextIsolation: true');
    expect(source).toContain('nodeIntegration: false');
    expect(source).not.toContain('preload:');
    expect(source).toContain('setPermissionCheckHandler(() => false)');
    expect(source).toContain("'http://*/*', 'https://*/*', 'file://*/*', 'ws://*/*', 'wss://*/*'");
    expect(source).toContain("setWindowOpenHandler(() => ({ action: 'deny' }))");
    expect(source).toContain("webContents.on('will-navigate'");
    expect(source).toContain('processSignaturePixels.toString()');
    expect(source).toContain('context.getImageData');
    expect(source).toContain("outputCanvas.toDataURL('image/png')");
    expect(source).toContain('sanitizeSignatureAppearanceAsset');
    expect(source).toContain('width: result.width');
    expect(source).toContain('height: result.height');
  });
});
