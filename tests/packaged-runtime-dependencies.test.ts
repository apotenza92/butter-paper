import { createRequire } from 'node:module';
import { resolve } from 'node:path';

const require = createRequire(import.meta.url);
const {
  verifyPackagedRuntimeDependencies,
} = require('../scripts/verify-packaged-runtime-dependencies.cjs') as {
  verifyPackagedRuntimeDependencies(inputPath: string): {
    braceExpansion: string;
    minimatch: string;
  };
};

describe('packaged updater runtime dependencies', () => {
  it('executes delegated TUF path matching with the reviewed patched dependencies', () => {
    expect(verifyPackagedRuntimeDependencies(resolve('apps/desktop'))).toEqual({
      braceExpansion: '5.0.9',
      minimatch: '10.2.6',
    });
  });
});
