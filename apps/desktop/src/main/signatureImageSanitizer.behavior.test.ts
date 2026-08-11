import { beforeEach, describe, expect, it, vi } from 'vitest';

const sanitizerHarness = vi.hoisted(() => ({
  executeResult: {
    dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
    width: 320,
    height: 120,
  } as unknown,
  executeJavaScript: vi.fn(),
  destroy: vi.fn(),
  clearStorageData: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('electron', () => {
  class TestBrowserWindow {
    webContents = {
      setWindowOpenHandler: vi.fn(),
      executeJavaScript: sanitizerHarness.executeJavaScript,
      on: vi.fn(),
    };
    loadURL = vi.fn().mockResolvedValue(undefined);
    isDestroyed = vi.fn(() => false);
    destroy = sanitizerHarness.destroy;
  }

  return {
    default: {
      BrowserWindow: TestBrowserWindow,
      session: {
        fromPartition: vi.fn(() => ({
          setPermissionCheckHandler: vi.fn(),
          setPermissionRequestHandler: vi.fn(),
          webRequest: { onBeforeRequest: vi.fn() },
          clearStorageData: sanitizerHarness.clearStorageData,
        })),
      },
    },
  };
});

import { sanitizeSignatureAppearanceAsset } from './signatureImageSanitizer';

describe('signature image sanitizer behavior', () => {
  beforeEach(() => {
    sanitizerHarness.executeResult = {
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      width: 320,
      height: 120,
    };
    sanitizerHarness.executeJavaScript.mockReset().mockImplementation(async () => sanitizerHarness.executeResult);
    sanitizerHarness.destroy.mockClear();
    sanitizerHarness.clearStorageData.mockClear();
  });

  it('uses decoded dimensions and PNG output from the isolated sanitizer', async () => {
    const result = await sanitizeSignatureAppearanceAsset({
      dataUrl: 'data:image/jpeg;base64,/9j/',
      mimeType: 'image/jpeg',
      width: 4000,
      height: 4000,
      source: 'image',
    });

    expect(result).toEqual({
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png',
      width: 320,
      height: 120,
      source: 'image',
    });
    expect(sanitizerHarness.destroy).toHaveBeenCalledOnce();
    expect(sanitizerHarness.clearStorageData).toHaveBeenCalledOnce();
  });

  it('rejects malformed isolated results and still cleans up', async () => {
    sanitizerHarness.executeResult = { dataUrl: 'not-an-image', width: 320, height: 120 };
    await expect(sanitizeSignatureAppearanceAsset({
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png',
      width: 640,
      height: 240,
      source: 'drawn',
    })).rejects.toThrow('returned an invalid result');
    expect(sanitizerHarness.destroy).toHaveBeenCalledOnce();
    expect(sanitizerHarness.clearStorageData).toHaveBeenCalledOnce();
  });

  it('rejects sanitized PNG output larger than the IPC image limit', async () => {
    sanitizerHarness.executeResult = {
      dataUrl: `data:image/png;base64,${'A'.repeat(14 * 1024 * 1024)}`,
      width: 4096,
      height: 4096,
    };

    await expect(sanitizeSignatureAppearanceAsset({
      dataUrl: 'data:image/jpeg;base64,/9j/',
      mimeType: 'image/jpeg',
      width: 4096,
      height: 4096,
      source: 'image',
    })).rejects.toThrow('larger than 10 MB');
    expect(sanitizerHarness.destroy).toHaveBeenCalledOnce();
    expect(sanitizerHarness.clearStorageData).toHaveBeenCalledOnce();
  });
});
