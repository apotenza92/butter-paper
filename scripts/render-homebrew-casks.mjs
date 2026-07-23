import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';

export function renderCasks(manifest) {
  if (!/^v\d+\.\d+\.\d+(?:-beta\.[1-9]\d*)?$/.test(manifest?.tag)
    || typeof manifest.version !== 'string'
    || manifest.tag.slice(1) !== manifest.version
    || manifest.channels == null) {
    throw new Error('Homebrew release manifest identity is invalid');
  }
  const entries = Object.entries(manifest.channels);
  if (entries.length === 0 || entries.some(([channel]) => !['stable', 'beta'].includes(channel))) {
    throw new Error('Homebrew release manifest channels are invalid');
  }
  return Object.fromEntries(entries.map(([channel, info]) => {
    for (const arch of ['arm64', 'x64']) {
      if (!/^[a-f0-9]{64}$/.test(info?.files?.[arch]?.sha256)
        || !String(info?.files?.[arch]?.url).startsWith('https://github.com/apotenza92/butter-paper/releases/download/')) {
        throw new Error(`Homebrew ${channel}/${arch} file metadata is invalid`);
      }
    }
    const beta = channel === 'beta';
    const token = beta ? 'butter-paper@beta' : 'butter-paper';
    const name = beta ? 'Butter Paper Beta' : 'Butter Paper';
    const appId = beta ? 'com.butterpaper.desktop.beta' : 'com.butterpaper.desktop';
    const architecture = (arch) => {
      const file = info.files[arch];
      return `  on_${arch === 'arm64' ? 'arm' : 'intel'} do\n    sha256 "${file.sha256}"\n    url "${file.url}"\n  end\n`;
    };
    const content = `cask "${token}" do\n  version "${manifest.version}"\n\n${architecture('arm64')}\n${architecture('x64')}\n  name "${name}"\n  desc "Cross-platform PDF review and markup${beta ? ' (beta channel)' : ''}"\n  homepage "https://github.com/apotenza92/butter-paper"\n\n  livecheck do\n    skip "Updated by the Butter Paper release workflow"\n  end\n\n  auto_updates true\n  app "${info.app}"\n\n  zap trash: [\n    "~/Library/Application Support/${name}",\n    "~/Library/Caches/${appId}",\n    "~/Library/Caches/${appId}.ShipIt",\n    "~/Library/Preferences/${appId}.plist",\n    "~/Library/Saved Application State/${appId}.savedState",\n  ]\nend\n`;
    return [`${token}.rb`, content];
  }));
}

export function main() {
  const manifestPath = resolve(process.argv[2] ?? 'homebrew-release.json');
  const outputDirectory = resolve(process.argv[3] ?? 'homebrew-tap/Casks');
  const casks = renderCasks(JSON.parse(readFileSync(manifestPath, 'utf8')));
  mkdirSync(outputDirectory, { recursive: true });
  for (const [filename, content] of Object.entries(casks)) {
    const outputPath = join(outputDirectory, filename);
    mkdirSync(dirname(outputPath), { recursive: true });
    writeFileSync(outputPath, content);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main();
}
