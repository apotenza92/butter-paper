import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';

const require = createRequire(import.meta.url);
const { extractReleaseNotes } = require('../scripts/release-notes.cjs') as {
  extractReleaseNotes(changelog: string, version: string): string;
};

describe('release notes', () => {
  it('extracts only the requested changelog section', () => {
    const changelog = `# Changelog

## [Unreleased]

- Future work.

## [1.2.3]

- Added the release feature.
- Fixed the release bug.

## [1.2.2]

- Previous work.
`;

    expect(extractReleaseNotes(changelog, '1.2.3')).toBe(
      '- Added the release feature.\n- Fixed the release bug.',
    );
  });

  it('rejects missing and empty release sections', () => {
    expect(() => extractReleaseNotes('## [1.0.0]\n', '2.0.0')).toThrow(
      'does not contain a [2.0.0] section',
    );
    expect(() => extractReleaseNotes('## [2.0.0]\n\n## [1.0.0]\n\n- Older.', '2.0.0')).toThrow(
      'section [2.0.0] is empty',
    );
  });

  it('uses the changelog for GitHub release notes', () => {
    const workflow = readFileSync(resolve('.github/workflows/release.yml'), 'utf8');
    expect(workflow).toContain('scripts/release-notes.cjs');
    expect(workflow).toContain('--notes-file release-notes.md');
    expect(workflow).not.toContain('--generate-notes');
  });
});
