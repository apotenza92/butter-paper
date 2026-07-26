import { resolvePkcs12OpenSsl } from '../scripts/resolve-openssl.mjs';

function result(help: string) {
  return {
    error: undefined,
    status: 0,
    signal: null,
    stdout: '',
    stderr: help,
    pid: 1,
    output: [null, '', help],
  };
}

describe('macOS signing OpenSSL resolution', () => {
  it('prefers an explicitly configured compatible OpenSSL binary', () => {
    const spawn = vi.fn(() => result('Options:\n -legacy Use legacy encryption\n'));
    expect(resolvePkcs12OpenSsl(
      { OPENSSL_BIN: '/tools/openssl' },
      { exists: () => true, spawn },
    )).toBe('/tools/openssl');
    expect(spawn).toHaveBeenCalledWith('/tools/openssl', ['pkcs12', '-help'], expect.any(Object));
  });

  it('skips system LibreSSL and selects the Homebrew OpenSSL 3 toolchain', () => {
    const spawn = vi.fn((command: string) => result(
      command === '/opt/homebrew/opt/openssl@3/bin/openssl'
        ? 'Options:\n -legacy Use legacy encryption\n'
        : 'LibreSSL pkcs12 help\n',
    ));
    expect(resolvePkcs12OpenSsl(
      {},
      {
        exists: (command: string) => command === '/opt/homebrew/opt/openssl@3/bin/openssl',
        spawn,
      },
    )).toBe('/opt/homebrew/opt/openssl@3/bin/openssl');
  });

  it('fails closed when no OpenSSL toolchain supports legacy PKCS#12 input', () => {
    expect(() => resolvePkcs12OpenSsl(
      {},
      {
        exists: () => false,
        spawn: () => result('LibreSSL pkcs12 help\n'),
      },
    )).toThrow(/OpenSSL 3 with legacy PKCS#12 support/);
  });
});
