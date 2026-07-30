import { describe, expect, it } from 'vitest';
import { assertIsolatedGuiTestEnvironment } from '../scripts/gui-test-environment.mjs';

describe('GUI test environment', () => {
  it('rejects GUI test launches on a local macOS desktop', () => {
    expect(() => assertIsolatedGuiTestEnvironment('Drawing test', {
      platform: 'darwin',
      githubActions: '',
    })).toThrow(/disabled on local macOS/);
  });

  it('allows disposable GitHub macOS runners and non-macOS native runners', () => {
    expect(() => assertIsolatedGuiTestEnvironment('Drawing test', {
      platform: 'darwin',
      githubActions: 'true',
    })).not.toThrow();
    expect(() => assertIsolatedGuiTestEnvironment('Drawing test', {
      platform: 'win32',
      githubActions: undefined,
    })).not.toThrow();
    expect(() => assertIsolatedGuiTestEnvironment('Drawing test', {
      platform: 'linux',
      githubActions: undefined,
    })).not.toThrow();
  });
});
