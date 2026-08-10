import { readFile } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

describe('signed hostile fixture contract', () => {
  it('covers every required hostile input and deterministic expected action', async () => {
    const contract = JSON.parse(await readFile(new URL('../../scripts/bluebeam-compat/signed-hostile-fixtures.json', import.meta.url), 'utf8'));
    const ids = contract.fixtures.map((fixture: { id: string }) => fixture.id);
    expect(ids).toEqual(expect.arrayContaining([
      'malformed-byte-range', 'corrupt-cms', 'changed-signed-bytes', 'prohibited-change',
      'output-collision', 'source-race', 'symlink-source-or-output', 'cancellation',
      'failed-postvalidation', 'secret-leakage', 'path-leakage',
    ]));
    expect(contract.fixtures.every((fixture: { expected: string }) => typeof fixture.expected === 'string')).toBe(true);
  });
});
