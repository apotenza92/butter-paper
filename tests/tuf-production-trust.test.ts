import { createHash, createPublicKey, verify } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { canonicalize } from '@tufjs/canonical-json';

const require = createRequire(import.meta.url);
const {
  ROLE_NAMES,
  createProductionTrust,
} = require('../scripts/create-tuf-production-trust.cjs') as {
  ROLE_NAMES: readonly string[];
  createProductionTrust(options: {
    privateKeyBundlePath: string;
    rootPath: string;
  }): {
    keyIDs: Record<string, string>;
    rootSha256: string;
  };
};

const ED25519_SPKI_PREFIX = Buffer.from('302a300506032b6570032100', 'hex');
const REVIEWED_ROOT_SHA256 = '9dd29ed74569f0f7e9288eaaed47118c66d71f58ba2f9d25242c6f304f2f12cb';

describe('production TUF trust ceremony', () => {
  it('locks the committed public root to the independently reviewed hash and signature', () => {
    const rootBytes = readFileSync(resolve('apps/desktop/build/update-trust/root.json'));
    const root = JSON.parse(rootBytes.toString('utf8'));
    expect(createHash('sha256').update(rootBytes).digest('hex')).toBe(REVIEWED_ROOT_SHA256);
    expect(rootBytes.toString('utf8')).not.toContain('PRIVATE KEY');
    expect(root.signed).toMatchObject({
      _type: 'root',
      spec_version: '1.0.31',
      version: 1,
    });
    const roleKeyIDs = ROLE_NAMES.map(role => root.signed.roles[role].keyids[0]);
    expect(new Set(roleKeyIDs).size).toBe(ROLE_NAMES.length);
    const rootKeyID = root.signed.roles.root.keyids[0];
    const rootKey = root.signed.keys[rootKeyID];
    const publicKey = createPublicKey({
      key: Buffer.concat([
        ED25519_SPKI_PREFIX,
        Buffer.from(rootKey.keyval.public, 'hex'),
      ]),
      format: 'der',
      type: 'spki',
    });
    expect(verify(
      null,
      Buffer.from(canonicalize(root.signed)),
      publicKey,
      Buffer.from(root.signatures[0].sig, 'hex'),
    )).toBe(true);
  });

  it('uses distinct role keys and a valid offline-root signature', () => {
    const temporary = mkdtempSync(join(tmpdir(), 'butter-paper-tuf-production-'));
    try {
      const privateKeyBundlePath = join(temporary, 'private', 'keys.json');
      const rootPath = join(temporary, 'public', 'root.json');
      const result = createProductionTrust({ privateKeyBundlePath, rootPath });
      const rootBytes = readFileSync(rootPath);
      const root = JSON.parse(rootBytes.toString('utf8'));
      const privateBundle = JSON.parse(readFileSync(privateKeyBundlePath, 'utf8'));

      expect(root.signed).toMatchObject({
        _type: 'root',
        consistent_snapshot: false,
        spec_version: '1.0.31',
        version: 1,
      });
      expect(Date.parse(root.signed.expires))
        .toBeGreaterThan(Date.now() + (5 * 365 * 24 * 60 * 60 * 1_000));
      expect(Object.keys(root.signed.roles).sort()).toEqual([...ROLE_NAMES].sort());
      expect(new Set(Object.values(result.keyIDs)).size).toBe(ROLE_NAMES.length);
      expect(createHash('sha256').update(rootBytes).digest('hex')).toBe(result.rootSha256);

      for (const role of ROLE_NAMES) {
        expect(root.signed.roles[role]).toEqual({
          keyids: [result.keyIDs[role]],
          threshold: 1,
        });
        expect(privateBundle.roles[role].keyid).toBe(result.keyIDs[role]);
        expect(privateBundle.roles[role].private_key_pem).toMatch(/^-----BEGIN PRIVATE KEY-----/);
      }
      expect(rootBytes.toString('utf8')).not.toContain('PRIVATE KEY');
      expect(statSync(privateKeyBundlePath).mode & 0o777).toBe(0o600);

      const rootKey = root.signed.keys[result.keyIDs.root!];
      const publicKey = createPublicKey({
        key: Buffer.concat([
          ED25519_SPKI_PREFIX,
          Buffer.from(rootKey.keyval.public, 'hex'),
        ]),
        format: 'der',
        type: 'spki',
      });
      expect(verify(
        null,
        Buffer.from(canonicalize(root.signed)),
        publicKey,
        Buffer.from(root.signatures[0].sig, 'hex'),
      )).toBe(true);
    } finally {
      rmSync(temporary, { recursive: true, force: true });
    }
  });

  it('refuses to overwrite public or private ceremony outputs', () => {
    const temporary = mkdtempSync(join(tmpdir(), 'butter-paper-tuf-production-'));
    try {
      const privateKeyBundlePath = join(temporary, 'keys.json');
      const rootPath = join(temporary, 'root.json');
      writeFileSync(rootPath, 'reviewed');
      expect(() => createProductionTrust({ privateKeyBundlePath, rootPath }))
        .toThrow(/must not already exist/);
      expect(readFileSync(rootPath, 'utf8')).toBe('reviewed');
    } finally {
      rmSync(temporary, { recursive: true, force: true });
    }
  });
});
