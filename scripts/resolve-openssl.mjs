import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';

const HOMEBREW_OPENSSL_CANDIDATES = [
  '/opt/homebrew/opt/openssl@3/bin/openssl',
  '/usr/local/opt/openssl@3/bin/openssl',
  '/opt/homebrew/bin/openssl',
  '/usr/local/bin/openssl',
];

function supportsLegacyPkcs12(command, spawn = spawnSync) {
  const result = spawn(command, ['pkcs12', '-help'], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  return result.error == null
    && `${result.stdout ?? ''}${result.stderr ?? ''}`.includes('-legacy');
}

export function resolvePkcs12OpenSsl(
  environment = process.env,
  {
    exists = existsSync,
    spawn = spawnSync,
  } = {},
) {
  const candidates = [
    environment.OPENSSL_BIN?.trim(),
    ...HOMEBREW_OPENSSL_CANDIDATES,
    'openssl',
  ].filter(Boolean);

  for (const candidate of [...new Set(candidates)]) {
    if (candidate.includes('/') && !exists(candidate)) {
      continue;
    }
    if (supportsLegacyPkcs12(candidate, spawn)) {
      return candidate;
    }
  }

  throw new Error('OpenSSL 3 with legacy PKCS#12 support is required for macOS release signing');
}
