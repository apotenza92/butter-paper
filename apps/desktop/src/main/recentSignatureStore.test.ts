import { mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { afterEach, describe, expect, it } from 'vitest';
import type { SignatureAppearanceAsset } from '@butter-paper/core';
import { RecentSignatureStore } from './recentSignatureStore';

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((directory) => rm(directory, { recursive: true, force: true })));
});

describe('RecentSignatureStore', () => {
  it('encrypts saved signatures and restores them', async () => {
    const { store, statePath, secureStorage } = await createStore();
    const asset = signatureAsset('A');

    const remembered = await store.remember(asset);
    const fileBytes = await readFile(statePath);

    expect(remembered.available).toBe(true);
    expect(remembered.signatures).toHaveLength(1);
    expect(fileBytes.includes(Buffer.from(asset.dataUrl))).toBe(false);
    const restartedStore = new RecentSignatureStore({ statePath, secureStorage });
    expect((await restartedStore.list()).signatures).toEqual(remembered.signatures);
  });

  it('moves reused signatures to the front without duplicates and keeps five', async () => {
    let now = 100;
    const { store } = await createStore({ now: () => now++ });
    for (const value of ['A', 'B', 'C', 'D', 'E', 'F']) await store.remember(signatureAsset(value));

    const snapshot = await store.remember(signatureAsset('C'));

    expect(snapshot.signatures).toHaveLength(5);
    expect(snapshot.signatures.map((entry) => entry.asset.dataUrl)).toEqual([
      signatureAsset('C').dataUrl,
      signatureAsset('F').dataUrl,
      signatureAsset('E').dataUrl,
      signatureAsset('D').dataUrl,
      signatureAsset('B').dataUrl,
    ]);
  });

  it('removes one signature or clears all signatures', async () => {
    const { store, statePath } = await createStore();
    const first = await store.remember(signatureAsset('A'));
    await store.remember(signatureAsset('B'));

    const removed = await store.remove(first.signatures[0]!.id);
    expect(removed.signatures.map((entry) => entry.asset.dataUrl)).toEqual([signatureAsset('B').dataUrl]);
    expect((await store.clear()).signatures).toEqual([]);
    await expect(readFile(statePath)).rejects.toMatchObject({ code: 'ENOENT' });
  });

  it('fails closed when encryption is unavailable', async () => {
    const { store, statePath } = await createStore({ available: false });

    expect(await store.list()).toEqual({ available: false, signatures: [] });
    await expect(store.remember(signatureAsset('A'))).rejects.toThrow('Secure signature storage is not available');
    await expect(readFile(statePath)).rejects.toMatchObject({ code: 'ENOENT' });
  });

  it('fails closed for the weak Linux basic_text backend', async () => {
    const { store, statePath } = await createStore({ platform: 'linux', backend: 'basic_text' });
    expect(await store.list()).toEqual({ available: false, signatures: [] });
    await expect(store.remember(signatureAsset('A'))).rejects.toThrow('Secure signature storage is not available');
    await expect(readFile(statePath)).rejects.toMatchObject({ code: 'ENOENT' });
  });

  it('can clear encrypted state after secure storage becomes unavailable', async () => {
    let available = true;
    const { store, statePath } = await createStore({ isAvailable: () => available });
    await store.remember(signatureAsset('A'));
    available = false;
    expect(await store.clear()).toEqual({ available: false, signatures: [] });
    await expect(readFile(statePath)).rejects.toMatchObject({ code: 'ENOENT' });
  });

  it('rewrites state when asynchronous safe storage requests key rotation', async () => {
    let shouldReEncrypt = false;
    let encryptCalls = 0;
    const { store, statePath, secureStorage } = await createStore({
      shouldReEncrypt: () => shouldReEncrypt,
      onEncrypt: () => { encryptCalls += 1; },
    });
    await store.remember(signatureAsset('A'));
    shouldReEncrypt = true;
    const restartedStore = new RecentSignatureStore({ statePath, secureStorage, platform: 'darwin' });
    await restartedStore.list();
    expect(encryptCalls).toBe(2);
  });

  it('sanitizes and rewrites signatures saved by schema version 1', async () => {
    const sanitized = { ...signatureAsset('Z'), width: 320, height: 120 };
    const initial = await createStore();
    await initial.store.remember(signatureAsset('A'));
    const persisted = JSON.parse(initial.decryptString(await readFile(initial.statePath)));
    persisted.schemaVersion = 1;
    await writeFile(initial.statePath, initial.encryptString(JSON.stringify(persisted)));

    const migratedStore = new RecentSignatureStore({
      statePath: initial.statePath,
      secureStorage: initial.secureStorage,
      sanitizeAsset: async () => sanitized,
    });
    expect((await migratedStore.list()).signatures[0]?.asset).toEqual(sanitized);
    expect(JSON.parse(initial.decryptString(await readFile(initial.statePath))).schemaVersion).toBe(2);
  });

  it('rejects malformed renderer data and does not overwrite corrupt encrypted state', async () => {
    const { store, statePath, encryptString } = await createStore();
    await expect(store.remember({ dataUrl: 'not-an-image' })).rejects.toThrow();
    const corruptBytes = encryptString('{broken');
    await import('node:fs/promises').then(({ writeFile }) => writeFile(statePath, corruptBytes));
    await expect(store.list()).rejects.toThrow('could not be read securely');
    await expect(store.remember(signatureAsset('A'))).rejects.toThrow('could not be read securely');
    expect(await readFile(statePath)).toEqual(corruptBytes);
  });

  it('does not overwrite decrypted state with an invalid schema', async () => {
    const { store, statePath, encryptString } = await createStore();
    const invalidBytes = encryptString('{"schemaVersion":2,"signatures":"invalid"}');
    await import('node:fs/promises').then(({ writeFile }) => writeFile(statePath, invalidBytes));
    await expect(store.remember(signatureAsset('A'))).rejects.toThrow('could not be read securely');
    expect(await readFile(statePath)).toEqual(invalidBytes);
  });

  it('rejects an oversized encrypted file before decryption', async () => {
    let decryptCalls = 0;
    const { store, statePath } = await createStore({ onDecrypt: () => { decryptCalls += 1; } });
    await import('node:fs/promises').then(({ writeFile }) => writeFile(statePath, ''));
    await import('node:fs/promises').then(({ truncate }) => truncate(statePath, 33 * 1024 * 1024 + 1));
    await expect(store.list()).rejects.toThrow('could not be read securely');
    expect(decryptCalls).toBe(0);
  });

  it('serializes concurrent writes without losing signatures', async () => {
    const { store } = await createStore();
    await Promise.all(['A', 'B', 'C'].map((value) => store.remember(signatureAsset(value))));
    expect((await store.list()).signatures).toHaveLength(3);
  });

  it('stores the isolated sanitizer result instead of renderer-claimed dimensions', async () => {
    const sanitized = { ...signatureAsset('Z'), width: 320, height: 120 };
    const { store } = await createStore({ sanitizeAsset: async () => sanitized });
    const snapshot = await store.remember({ ...signatureAsset('A'), width: 4000, height: 4000 });
    expect(snapshot.signatures[0]?.asset).toEqual(sanitized);
  });

  it('removes stale encrypted temporary files', async () => {
    const { store, statePath } = await createStore();
    const stalePath = `${statePath}.123.456.1.tmp`;
    await import('node:fs/promises').then(({ writeFile }) => writeFile(stalePath, 'encrypted'));
    await store.clear();
    await expect(readFile(stalePath)).rejects.toMatchObject({ code: 'ENOENT' });
  });

  it('does not follow a pre-created temporary-file symlink', async () => {
    const token = 'fixed-token';
    const { store, statePath } = await createStore({ temporaryToken: () => token });
    const targetPath = `${statePath}.target`;
    const temporaryPath = `${statePath}.${token}.tmp`;
    await writeFile(targetPath, 'preserve me');
    await symlink(targetPath, temporaryPath);
    await expect(store.remember(signatureAsset('A'))).rejects.toMatchObject({ code: 'EEXIST' });
    expect(await readFile(targetPath, 'utf8')).toBe('preserve me');
  });

  it('bounds the pending operation queue', async () => {
    const { store } = await createStore();
    const results = await Promise.allSettled(['A', 'B', 'C', 'D'].map((value) => store.remember(signatureAsset(value))));
    expect(results.filter((result) => result.status === 'rejected')).toHaveLength(1);
    expect(results.find((result) => result.status === 'rejected')).toMatchObject({
      reason: expect.objectContaining({ message: expect.stringContaining('Too many') }),
    });
  });
});

async function createStore(options: {
  available?: boolean;
  isAvailable?: () => boolean;
  backend?: string;
  now?: () => number;
  onDecrypt?: () => void;
  onEncrypt?: () => void;
  platform?: NodeJS.Platform;
  sanitizeAsset?: (asset: SignatureAppearanceAsset) => Promise<SignatureAppearanceAsset>;
  shouldReEncrypt?: () => boolean;
  temporaryToken?: () => string;
} = {}) {
  const directory = await mkdtemp(join(tmpdir(), 'butter-paper-recent-signatures-'));
  temporaryDirectories.push(directory);
  const statePath = join(directory, 'recent-signatures.v1.enc');
  const encryptString = (value: string) => Buffer.from(
    Buffer.from(value, 'utf8').map((byte) => byte ^ 0x5a),
  );
  const decryptString = (value: Buffer) => Buffer.from(
    value.map((byte) => byte ^ 0x5a),
  ).toString('utf8');
  const secureStorage = {
    isEncryptionAvailable: () => options.isAvailable?.() ?? options.available ?? true,
    isAsyncEncryptionAvailable: async () => options.isAvailable?.() ?? options.available ?? true,
    encryptString: (value: string) => {
      options.onEncrypt?.();
      return encryptString(value);
    },
    encryptStringAsync: async (value: string) => {
      options.onEncrypt?.();
      return encryptString(value);
    },
    decryptString: (value: Buffer) => {
      options.onDecrypt?.();
      return decryptString(value);
    },
    decryptStringAsync: async (value: Buffer) => {
      options.onDecrypt?.();
      return { result: decryptString(value), shouldReEncrypt: options.shouldReEncrypt?.() ?? false };
    },
    getSelectedStorageBackend: () => options.backend ?? 'unknown',
  };
  const store = new RecentSignatureStore({
    statePath,
    now: options.now,
    platform: options.platform,
    sanitizeAsset: options.sanitizeAsset,
    temporaryToken: options.temporaryToken,
    secureStorage,
  });
  return { store, statePath, encryptString, decryptString, secureStorage };
}

function signatureAsset(value: string): SignatureAppearanceAsset {
  return {
    dataUrl: `data:image/png;base64,iVBORw0KGgo${value}`,
    mimeType: 'image/png',
    width: 640,
    height: 240,
    source: 'drawn',
  };
}
