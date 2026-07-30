import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const require = createRequire(import.meta.url);
const {
  createProductionTrust,
} = require('../scripts/create-tuf-production-trust.cjs') as {
  createProductionTrust(options: {
    privateKeyBundlePath: string;
    rootPath: string;
  }): { rootSha256: string };
};
const {
  signUpdateRepository,
  verifyEnvelope,
} = require('../scripts/sign-tuf-update-repository.cjs') as {
  signUpdateRepository(options: {
    now?: Date;
    outputDirectory: string;
    previousMetadataDirectory: string | null;
    privateKeys: Record<string, string>;
    rootPath: string;
    targetName: string;
    targetPath: string;
  }): { versions: Record<string, number> };
  verifyEnvelope(envelope: unknown, root: unknown, roleName: string): void;
};

function fixture() {
  const temporary = mkdtempSync(join(tmpdir(), 'butter-paper-tuf-signing-'));
  const privateKeyBundlePath = join(temporary, 'private-keys.json');
  const rootPath = join(temporary, 'root.json');
  createProductionTrust({ privateKeyBundlePath, rootPath });
  const privateBundle = JSON.parse(readFileSync(privateKeyBundlePath, 'utf8'));
  return {
    cleanup: () => rmSync(temporary, { recursive: true, force: true }),
    privateKeys: Object.fromEntries(
      ['targets', 'snapshot', 'timestamp'].map(role => [
        role,
        privateBundle.roles[role].private_key_pem,
      ]),
    ),
    rootPath,
    temporary,
  };
}

describe('production TUF metadata signing', () => {
  it('creates distinct, verifiable, expiring online metadata', () => {
    const { cleanup, privateKeys, rootPath, temporary } = fixture();
    try {
      const targetPath = join(temporary, 'latest.yml');
      const outputDirectory = join(temporary, 'repository-v1');
      writeFileSync(targetPath, 'version: 0.0.12\n');
      const result = signUpdateRepository({
        now: new Date('2026-07-30T00:00:00Z'),
        outputDirectory,
        previousMetadataDirectory: null,
        privateKeys,
        rootPath,
        targetName: 'latest.yml',
        targetPath,
      });
      const root = JSON.parse(readFileSync(rootPath, 'utf8'));
      const targets = JSON.parse(readFileSync(join(outputDirectory, 'metadata', 'targets.json'), 'utf8'));
      const snapshot = JSON.parse(readFileSync(join(outputDirectory, 'metadata', 'snapshot.json'), 'utf8'));
      const timestamp = JSON.parse(readFileSync(join(outputDirectory, 'metadata', 'timestamp.json'), 'utf8'));

      expect(result.versions).toEqual({ targets: 1, snapshot: 1, timestamp: 1 });
      expect(() => verifyEnvelope(targets, root, 'targets')).not.toThrow();
      expect(() => verifyEnvelope(snapshot, root, 'snapshot')).not.toThrow();
      expect(() => verifyEnvelope(timestamp, root, 'timestamp')).not.toThrow();
      expect(readFileSync(join(outputDirectory, 'targets', 'latest.yml'), 'utf8'))
        .toBe('version: 0.0.12\n');
      expect(readFileSync(join(outputDirectory, 'EVIDENCE.txt'), 'utf8'))
        .toContain('Target SHA-256:');
      expect(readFileSync(join(outputDirectory, 'metadata', 'root.json'), 'utf8'))
        .not.toContain('PRIVATE KEY');
    } finally {
      cleanup();
    }
  });

  it('increments verified prior metadata and rejects tampering', () => {
    const { cleanup, privateKeys, rootPath, temporary } = fixture();
    try {
      const targetPath = join(temporary, 'latest.yml');
      const first = join(temporary, 'repository-v1');
      const second = join(temporary, 'repository-v2');
      writeFileSync(targetPath, 'version: 0.0.11\n');
      signUpdateRepository({
        now: new Date('2026-07-29T00:00:00Z'),
        outputDirectory: first,
        previousMetadataDirectory: null,
        privateKeys,
        rootPath,
        targetName: 'latest.yml',
        targetPath,
      });
      writeFileSync(targetPath, 'version: 0.0.12\n');
      const result = signUpdateRepository({
        now: new Date('2026-07-30T00:00:00Z'),
        outputDirectory: second,
        previousMetadataDirectory: join(first, 'metadata'),
        privateKeys,
        rootPath,
        targetName: 'latest.yml',
        targetPath,
      });
      expect(result.versions).toEqual({ targets: 2, snapshot: 2, timestamp: 2 });

      const targetsPath = join(first, 'metadata', 'targets.json');
      const tampered = JSON.parse(readFileSync(targetsPath, 'utf8'));
      tampered.signed.version = 99;
      writeFileSync(targetsPath, JSON.stringify(tampered));
      expect(() => signUpdateRepository({
        now: new Date('2026-07-31T00:00:00Z'),
        outputDirectory: join(temporary, 'repository-tampered'),
        previousMetadataDirectory: join(first, 'metadata'),
        privateKeys,
        rootPath,
        targetName: 'latest.yml',
        targetPath,
      })).toThrow(/signature threshold/);
    } finally {
      cleanup();
    }
  });

  it('rejects online keys that do not match their reviewed roles', () => {
    const { cleanup, privateKeys, rootPath, temporary } = fixture();
    try {
      const targetPath = join(temporary, 'latest.yml');
      writeFileSync(targetPath, 'version: 0.0.12\n');
      expect(() => signUpdateRepository({
        outputDirectory: join(temporary, 'repository'),
        previousMetadataDirectory: null,
        privateKeys: { ...privateKeys, targets: privateKeys.timestamp! },
        rootPath,
        targetName: 'latest.yml',
        targetPath,
      })).toThrow(/targets key does not match/);
    } finally {
      cleanup();
    }
  });
});
