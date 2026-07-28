const releasePageUrl = 'https://github.com/apotenza92/butter-paper/releases';

export function resolveReleasePageUrl(availableVersion: string | null): string {
  return availableVersion
    ? `${releasePageUrl}/tag/v${encodeURIComponent(availableVersion)}`
    : releasePageUrl;
}
