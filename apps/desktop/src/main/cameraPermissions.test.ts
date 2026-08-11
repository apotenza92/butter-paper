import type { BrowserWindow, PermissionCheckHandlerHandlerDetails } from 'electron';
import { describe, expect, it, vi } from 'vitest';
import { configureCameraPermissions } from './cameraPermissions';

describe('camera permission policy', () => {
  it('permits only main-frame video requests from the application window', () => {
    type CheckHandler = Exclude<Parameters<BrowserWindow['webContents']['session']['setPermissionCheckHandler']>[0], null>;
    type RequestHandler = Exclude<Parameters<BrowserWindow['webContents']['session']['setPermissionRequestHandler']>[0], null>;
    let checkHandler: CheckHandler | undefined;
    let requestHandler: RequestHandler | undefined;
    const session = {
      setPermissionCheckHandler: vi.fn((handler) => { checkHandler = handler; }),
      setPermissionRequestHandler: vi.fn((handler) => { requestHandler = handler; }),
    };
    const webContents = { session };
    configureCameraPermissions({ webContents } as unknown as BrowserWindow, 'file:///app/index.html');
    const checkDetails = {
      isMainFrame: true,
      mediaType: 'video',
      requestingUrl: 'file:///app/index.html',
    } satisfies PermissionCheckHandlerHandlerDetails;
    if (!checkHandler || !requestHandler) throw new Error('Permission handlers were not installed.');

    expect(checkHandler(webContents as never, 'media', 'file://', checkDetails)).toBe(true);
    expect(checkHandler(webContents as never, 'media', 'file://', { ...checkDetails, mediaType: 'audio' })).toBe(false);
    expect(checkHandler(webContents as never, 'geolocation', 'file://', checkDetails)).toBe(false);
    expect(checkHandler(webContents as never, 'media', 'file://', { ...checkDetails, isMainFrame: false })).toBe(false);
    expect(checkHandler(webContents as never, 'media', 'file://', {
      ...checkDetails,
      requestingUrl: 'https://example.com/',
    })).toBe(false);
    expect(checkHandler(null, 'media', 'file://', checkDetails)).toBe(false);

    const callback = vi.fn();
    requestHandler(webContents as never, 'media', callback, {
      isMainFrame: true,
      mediaTypes: ['video'],
      requestingUrl: 'file:///app/index.html',
    });
    expect(callback).toHaveBeenLastCalledWith(true);
    requestHandler(webContents as never, 'media', callback, {
      isMainFrame: true,
      mediaTypes: ['video', 'audio'],
      requestingUrl: 'file:///app/index.html',
    });
    expect(callback).toHaveBeenLastCalledWith(false);
    requestHandler(webContents as never, 'notifications', callback, {
      isMainFrame: true,
      requestingUrl: 'file:///app/index.html',
    });
    expect(callback).toHaveBeenLastCalledWith(false);
    requestHandler(webContents as never, 'media', callback, {
      isMainFrame: true,
      mediaTypes: ['video'],
      requestingUrl: 'https://example.com/',
    });
    expect(callback).toHaveBeenLastCalledWith(false);
  });
});
