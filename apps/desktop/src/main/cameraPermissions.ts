import type { BrowserWindow } from 'electron';

export function configureCameraPermissions(window: BrowserWindow, rendererUrl: string): void {
  const applicationWebContents = window.webContents;
  const applicationSession = applicationWebContents.session;
  const isExpectedRendererSource = (candidate: string | undefined): boolean => {
    if (!candidate) return false;
    try {
      const expected = new URL(rendererUrl);
      const requested = new URL(candidate);
      if (expected.protocol === 'file:') {
        return requested.protocol === 'file:' && requested.pathname === expected.pathname;
      }
      return requested.origin === expected.origin;
    } catch {
      return false;
    }
  };

  applicationSession.setPermissionCheckHandler((webContents, permission, requestingOrigin, details) => (
    webContents === applicationWebContents
    && permission === 'media'
    && details.isMainFrame
    && details.mediaType === 'video'
    && isExpectedRendererSource(details.requestingUrl ?? requestingOrigin)
  ));

  applicationSession.setPermissionRequestHandler((webContents, permission, callback, details) => {
    const mediaTypes = 'mediaTypes' in details ? details.mediaTypes : undefined;
    callback(
      webContents === applicationWebContents
      && permission === 'media'
      && details.isMainFrame
      && mediaTypes?.length === 1
      && mediaTypes[0] === 'video'
      && isExpectedRendererSource(details.requestingUrl),
    );
  });
}
