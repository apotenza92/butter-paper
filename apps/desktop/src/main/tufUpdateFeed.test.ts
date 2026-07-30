import { createHash, generateKeyPairSync, sign, type KeyObject } from 'node:crypto';
import { once } from 'node:events';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { canonicalize } from '@tufjs/canonical-json';
import {
  createTufVerifiedUpdateFeed,
  initializeTrustedRoot,
  validateAuthenticatedUpdateChannel,
  validateTufRepositoryUrl,
  validateTufTargetName,
  type TufVerifiedUpdateFeed,
} from './tufUpdateFeed';

interface TufFixture {
  metadata: Record<string, Buffer>;
  targetBytes: Buffer;
  targetName: string;
}

function sha256(bytes: Buffer): string {
  return createHash('sha256').update(bytes).digest('hex');
}

function signedMetadata(signed: object, keyID: string, privateKey: KeyObject) {
  return {
    signatures: [{
      keyid: keyID,
      sig: sign(null, Buffer.from(canonicalize(signed)), privateKey).toString('hex'),
    }],
    signed,
  };
}

function tufFixture(options: {
  expires?: string;
  targetName?: string;
  wrongTargetsSignature?: boolean;
  snapshotVersionMismatch?: boolean;
} = {}): TufFixture {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const publicDer = publicKey.export({ format: 'der', type: 'spki' });
  const key = {
    keytype: 'ed25519',
    scheme: 'ed25519',
    keyval: { public: publicDer.subarray(-32).toString('hex') },
  };
  const keyID = sha256(Buffer.from(canonicalize(key)));
  const expires = options.expires ?? '2035-01-01T00:00:00Z';
  const role = { keyids: [keyID], threshold: 1 };
  const targetName = options.targetName ?? 'latest.yml';
  const targetBytes = Buffer.from('version: 0.0.12\nbutterPaperChannel: stable\nfiles: []\n');
  const targetsSigner = options.wrongTargetsSignature
    ? generateKeyPairSync('ed25519').privateKey
    : privateKey;
  const targets = signedMetadata({
    _type: 'targets',
    spec_version: '1.0.31',
    version: 1,
    expires,
    targets: {
      [targetName]: {
        length: targetBytes.length,
        hashes: { sha256: sha256(targetBytes) },
      },
    },
  }, keyID, targetsSigner);
  const targetsBytes = Buffer.from(JSON.stringify(targets));
  const snapshot = signedMetadata({
    _type: 'snapshot',
    spec_version: '1.0.31',
    version: 1,
    expires,
    meta: {
      'targets.json': {
        version: options.snapshotVersionMismatch ? 2 : 1,
        length: targetsBytes.length,
        hashes: { sha256: sha256(targetsBytes) },
      },
    },
  }, keyID, privateKey);
  const snapshotBytes = Buffer.from(JSON.stringify(snapshot));
  const timestamp = signedMetadata({
    _type: 'timestamp',
    spec_version: '1.0.31',
    version: 1,
    expires,
    meta: {
      'snapshot.json': {
        version: 1,
        length: snapshotBytes.length,
        hashes: { sha256: sha256(snapshotBytes) },
      },
    },
  }, keyID, privateKey);
  const root = signedMetadata({
    _type: 'root',
    spec_version: '1.0.31',
    version: 1,
    expires,
    consistent_snapshot: false,
    keys: { [keyID]: key },
    roles: {
      root: role,
      snapshot: role,
      targets: role,
      timestamp: role,
    },
  }, keyID, privateKey);
  return {
    metadata: {
      'root.json': Buffer.from(JSON.stringify(root)),
      'snapshot.json': snapshotBytes,
      'targets.json': targetsBytes,
      'timestamp.json': Buffer.from(JSON.stringify(timestamp)),
    },
    targetBytes,
    targetName,
  };
}

async function fixtureServer(
  fixture: TufFixture,
  options: { missing?: string; redirect?: string } = {},
): Promise<{ close(): Promise<void>; server: Server; url: string }> {
  const server = createServer((request, response) => {
    const match = request.url?.match(/^\/(metadata|targets)\/([^/?]+)$/);
    if (!match) {
      response.writeHead(404).end();
      return;
    }
    if (match[1] === 'metadata' && match[2] === '2.root.json') {
      response.writeHead(404).end();
      return;
    }
    if (match[2] === options.redirect) {
      response.writeHead(302, { Location: `/redirected/${match[2]}` }).end();
      return;
    }
    if (match[2] === options.missing) {
      response.writeHead(404).end();
      return;
    }
    const bytes = match[1] === 'metadata'
      ? fixture.metadata[match[2]]
      : match[2] === fixture.targetName ? fixture.targetBytes : undefined;
    if (bytes == null) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, { 'Content-Length': bytes.length });
    response.end(bytes);
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  if (address == null || typeof address === 'string') {
    throw new Error('Fixture server did not expose a TCP port.');
  }
  return {
    close: () => new Promise((resolve, reject) => {
      server.close(error => error ? reject(error) : resolve());
    }),
    server,
    url: `http://127.0.0.1:${address.port}`,
  };
}

describe('TUF repository boundary', () => {
  it('requires HTTPS in production and rejects unsafe target names', () => {
    expect(validateTufRepositoryUrl('https://updates.example/butter-paper/'))
      .toBe('https://updates.example/butter-paper');
    expect(() => validateTufRepositoryUrl('http://updates.example/butter-paper'))
      .toThrow(/must use HTTPS/);
    expect(() => validateTufRepositoryUrl('https://user:secret@updates.example/feed'))
      .toThrow(/credentials/);
    expect(validateTufRepositoryUrl(
      'http://127.0.0.1:4317/repository',
      { allowLoopbackHttp: true },
    )).toBe('http://127.0.0.1:4317/repository');
    expect(validateTufTargetName('latest.yml')).toBe('latest.yml');
    expect(() => validateTufTargetName('../latest.yml')).toThrow(/Unsafe/);
    expect(() => validateTufTargetName('nested/latest.yml')).toThrow(/Unsafe/);
  });

  it('requires exactly one authenticated stable or beta channel declaration', () => {
    expect(() => validateAuthenticatedUpdateChannel(
      Buffer.from('version: 0.0.12\nbutterPaperChannel: stable\n'),
      'stable',
    )).not.toThrow();
    expect(() => validateAuthenticatedUpdateChannel(
      Buffer.from('version: 0.0.12\nbutterPaperChannel: beta\n'),
      'stable',
    )).toThrow(/expected stable/);
    expect(() => validateAuthenticatedUpdateChannel(
      Buffer.from('version: 0.0.12\n'),
      'stable',
    )).toThrow(/missing or duplicated/);
    expect(() => validateAuthenticatedUpdateChannel(
      Buffer.from('butterPaperChannel: stable\nbutterPaperChannel: stable\n'),
      'stable',
    )).toThrow(/missing or duplicated/);
  });

  it('initializes trust once and never replaces an advanced persisted root', () => {
    const root = mkdtempSync(join(tmpdir(), 'butter-paper-tuf-root-'));
    try {
      const embeddedRootPath = join(root, 'embedded.json');
      const metadataDirectory = join(root, 'metadata');
      writeFileSync(embeddedRootPath, 'root version one');
      const first = initializeTrustedRoot({ embeddedRootPath, metadataDirectory });
      expect(first.initialized).toBe(true);
      writeFileSync(first.trustedRootPath, 'advanced root version two');
      writeFileSync(embeddedRootPath, 'older replacement root');
      const second = initializeTrustedRoot({ embeddedRootPath, metadataDirectory });
      expect(second.initialized).toBe(false);
      expect(readFileSync(second.trustedRootPath, 'utf8')).toBe('advanced root version two');
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('serves only update metadata authenticated by TUF and closes idempotently', async () => {
    const root = mkdtempSync(join(tmpdir(), 'butter-paper-tuf-feed-'));
    const fixture = tufFixture();
    const repository = await fixtureServer(fixture);
    let feed: TufVerifiedUpdateFeed | null = null;
    try {
      const embeddedRootPath = join(root, 'embedded.json');
      writeFileSync(embeddedRootPath, fixture.metadata['root.json']!);
      feed = await createTufVerifiedUpdateFeed({
        embeddedRootPath,
        expectedChannel: 'stable',
        repositoryUrl: repository.url,
        targetName: fixture.targetName,
        trustDirectory: join(root, 'trust'),
        allowLoopbackHttp: true,
      });
      expect(feed.trustInitialized).toBe(true);
      const response = await fetch(`${feed.feedUrl}/${fixture.targetName}`);
      expect(response.status).toBe(200);
      expect(Buffer.from(await response.arrayBuffer())).toEqual(fixture.targetBytes);
      expect((await fetch(`${feed.feedUrl}/unexpected.yml`)).status).toBe(404);
      await feed.refresh();
      await feed.close();
      await feed.close();
      feed = null;
    } finally {
      await feed?.close();
      await repository.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  it.each([
    {
      label: 'corrupt target bytes',
      mutate(fixture: TufFixture) {
        fixture.targetBytes = Buffer.from('tampered update metadata');
      },
      server: {},
    },
    {
      label: 'expired metadata',
      fixture: { expires: '2020-01-01T00:00:00Z' },
      server: {},
    },
    {
      label: 'incorrect target signature',
      fixture: { wrongTargetsSignature: true },
      server: {},
    },
    {
      label: 'missing snapshot metadata',
      server: { missing: 'snapshot.json' },
    },
    {
      label: 'redirected timestamp metadata',
      server: { redirect: 'timestamp.json' },
    },
    {
      label: 'mismatched snapshot metadata',
      fixture: { snapshotVersionMismatch: true },
      server: {},
    },
  ])('fails closed on $label', async ({ fixture: fixtureOptions, mutate, server: serverOptions }) => {
    const root = mkdtempSync(join(tmpdir(), 'butter-paper-tuf-rejection-'));
    const fixture = tufFixture(fixtureOptions);
    mutate?.(fixture);
    const repository = await fixtureServer(fixture, serverOptions);
    try {
      const embeddedRootPath = join(root, 'embedded.json');
      writeFileSync(embeddedRootPath, fixture.metadata['root.json']!);
      await expect(createTufVerifiedUpdateFeed({
        embeddedRootPath,
        expectedChannel: 'stable',
        repositoryUrl: repository.url,
        targetName: fixture.targetName,
        trustDirectory: join(root, 'trust'),
        allowLoopbackHttp: true,
      })).rejects.toThrow();
    } finally {
      await repository.close();
      rmSync(root, { recursive: true, force: true });
    }
  });
});
