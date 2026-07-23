import { readFileSync } from 'node:fs';

describe('repository hygiene release guardrails', () => {
  it('mechanically rejects the obsolete package workflow and retired credential names', () => {
    const hygiene = readFileSync('scripts/check-repository-hygiene.mjs', 'utf8');
    expect(hygiene).toContain("'.github/workflows/packages.yml'");
    for (const retired of [
      'MACOS_CSC_LINK',
      'CSC_LINK',
      'APPLE_ID',
      'APPLE_APP_SPECIFIC_PASSWORD',
      'APPLE_ID_PASSWORD',
    ]) {
      expect(hygiene).toContain(retired);
    }
  });

  it('checks maintained package and workflow command paths for dangling references', () => {
    const hygiene = readFileSync('scripts/check-repository-hygiene.mjs', 'utf8');
    expect(hygiene).toContain('references missing maintained path');
    expect(hygiene).toContain("filePath.startsWith('.github/workflows/')");
    expect(hygiene).toContain("filePath === 'apps/desktop/package.json'");
    expect(hygiene).toContain('third-party action is not pinned to a full commit SHA');
    expect(hygiene).toContain('/@[a-f0-9]{40}$/');
  });
});
