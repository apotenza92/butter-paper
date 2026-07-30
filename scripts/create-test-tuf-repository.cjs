#!/usr/bin/env node

const { createHash, generateKeyPairSync, sign } = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { canonicalize } = require('@tufjs/canonical-json');

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function publicKeyDescription(publicKey) {
  const publicDer = publicKey.export({ format: 'der', type: 'spki' });
  return {
    keytype: 'ed25519',
    scheme: 'ed25519',
    keyval: { public: publicDer.subarray(-32).toString('hex') },
  };
}

function signedMetadata(signed, keyID, privateKey) {
  return {
    signatures: [{
      keyid: keyID,
      sig: sign(null, Buffer.from(canonicalize(signed)), privateKey).toString('hex'),
    }],
    signed,
  };
}

function createTestTrust({ privateKeyPath, rootPath }) {
  if (fs.existsSync(privateKeyPath) || fs.existsSync(rootPath)) {
    throw new Error('Test TUF trust outputs must not already exist.');
  }
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const key = publicKeyDescription(publicKey);
  const keyID = sha256(Buffer.from(canonicalize(key)));
  const role = { keyids: [keyID], threshold: 1 };
  const root = signedMetadata({
    _type: 'root',
    spec_version: '1.0.31',
    version: 1,
    expires: '2035-01-01T00:00:00Z',
    consistent_snapshot: false,
    keys: { [keyID]: key },
    roles: {
      root: role,
      snapshot: role,
      targets: role,
      timestamp: role,
    },
  }, keyID, privateKey);
  fs.mkdirSync(path.dirname(privateKeyPath), { recursive: true, mode: 0o700 });
  fs.mkdirSync(path.dirname(rootPath), { recursive: true });
  fs.writeFileSync(
    privateKeyPath,
    privateKey.export({ format: 'pem', type: 'pkcs8' }),
    { flag: 'wx', mode: 0o600 },
  );
  fs.writeFileSync(rootPath, `${JSON.stringify(root)}\n`, { flag: 'wx', mode: 0o644 });
  return { keyID, root };
}

function createTestRepositoryMetadata({
  privateKeyPath,
  rootPath,
  targetBytes,
  targetName,
  expires = new Date(Date.now() + 24 * 60 * 60 * 1_000).toISOString(),
}) {
  const privateKey = fs.readFileSync(privateKeyPath, 'utf8');
  const root = JSON.parse(fs.readFileSync(rootPath, 'utf8'));
  const publicKey = require('node:crypto').createPublicKey(privateKey);
  const key = publicKeyDescription(publicKey);
  const keyID = sha256(Buffer.from(canonicalize(key)));
  if (
    root?.signed?._type !== 'root'
    || !root.signed.keys?.[keyID]
    || !root.signed.roles?.targets?.keyids?.includes(keyID)
  ) {
    throw new Error('The ephemeral private key does not match the test TUF root.');
  }

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
  }, keyID, privateKey);
  const targetsBytes = Buffer.from(JSON.stringify(targets));
  const snapshot = signedMetadata({
    _type: 'snapshot',
    spec_version: '1.0.31',
    version: 1,
    expires,
    meta: {
      'targets.json': {
        version: 1,
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
  return {
    'root.json': Buffer.from(JSON.stringify(root)),
    'snapshot.json': snapshotBytes,
    'targets.json': targetsBytes,
    'timestamp.json': Buffer.from(JSON.stringify(timestamp)),
  };
}

function option(argv, name) {
  const index = argv.indexOf(name);
  return index >= 0 ? argv[index + 1] : null;
}

function main(argv = process.argv.slice(2)) {
  const privateKeyPath = option(argv, '--private-key');
  const rootPath = option(argv, '--root');
  if (!privateKeyPath || !rootPath) {
    throw new Error('Usage: create-test-tuf-repository.cjs --root <new-file> --private-key <new-file>');
  }
  createTestTrust({
    privateKeyPath: path.resolve(privateKeyPath),
    rootPath: path.resolve(rootPath),
  });
  process.stdout.write('Created ephemeral loopback-only Butter Paper TUF test trust.\n');
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}

module.exports = {
  createTestRepositoryMetadata,
  createTestTrust,
};
