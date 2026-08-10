import { readFileSync } from 'node:fs';
import { JSDOM } from 'jsdom';

const html = readFileSync('docs/index.html', 'utf8');
const readme = readFileSync('README.md', 'utf8');
const betaIcon = readFileSync('assets/butter-paper-icon-beta.svg', 'utf8');
const productDescription = 'Butter Paper is a free, open-source, cross-platform alternative to Bluebeam Revu for PDF review and markup in architecture, engineering, and construction—available on macOS, Windows, and Linux.';
const readmeProductDescription = readme.match(/<\/p>\n\n([\s\S]*?)\n\n/)?.[1].replaceAll('\n', ' ');

async function loadPage({
  architecture = '',
  platform = '',
  releases = null as null | {
    stable: { assets: Array<{ name: string; size: number }>; tag_name: string };
    beta?: { assets: Array<{ name: string; size: number }>; prerelease: boolean; tag_name: string };
  },
} = {}) {
  const dom = new JSDOM(html, {
    beforeParse(window) {
      Object.defineProperty(window.navigator, 'platform', { configurable: true, value: platform });
      Object.defineProperty(window.navigator, 'userAgent', { configurable: true, value: '' });
      Object.defineProperty(window.navigator, 'userAgentData', {
        configurable: true,
        value: platform || architecture ? { architecture, platform } : undefined,
      });
      if (releases) {
        window.fetch = (async (url: string | URL | Request) => {
          const data = String(url).endsWith('/releases/latest')
            ? releases.stable
            : releases.beta ? [releases.beta] : [];
          return { json: async () => data, ok: true } as Response;
        }) as typeof fetch;
      }
    },
    runScripts: 'dangerously',
    url: 'https://apotenza92.github.io/butter-paper/',
  });

  await new Promise((resolveTick) => dom.window.setTimeout(resolveTick, 0));
  return dom;
}

function hero(dom: JSDOM) {
  const document = dom.window.document;
  return {
    href: document.getElementById('hero-download-button')!.getAttribute('href'),
    label: document.getElementById('hero-download-label')!.textContent,
    link: document.getElementById('hero-download-button')!,
  };
}

describe('Butter Paper download page', () => {
  it('keeps the product description and download destination aligned with the README', async () => {
    const dom = await loadPage();
    expect(readmeProductDescription).toBe(productDescription);
    expect(dom.window.document.querySelector('.subtitle')!.textContent).toBe(readmeProductDescription);
    expect(dom.window.document.querySelector('meta[name="description"]')!.getAttribute('content')).toBe(readmeProductDescription);
    expect(dom.window.document.getElementById('app-icon')!.getAttribute('src')).toBe('./assets/icons/icon.png');
    expect(dom.window.document.getElementById('favicon')!.getAttribute('href')).toBe('./assets/icons/icon.png');
    expect(readme).toContain('https://apotenza92.github.io/butter-paper/');
  });

  it('positions Butter Paper as a free cross-platform Bluebeam Revu alternative', async () => {
    const dom = await loadPage();
    const copy = [
      dom.window.document.querySelector('meta[name="description"]')!.getAttribute('content'),
      dom.window.document.querySelector('.subtitle')!.textContent,
      readme,
    ].join(' ');

    expect(copy).toContain('free');
    expect(copy).toContain('cross-platform');
    expect(copy).toContain('Bluebeam Revu');
    expect(copy).toContain('macOS');
    expect(copy).toContain('Windows');
    expect(copy).toContain('Linux');
  });

  it.each([
    ['Windows', 'x86', 'Download Butter Paper for Windows x64', 'Butter-Paper-Windows-x64-Setup.exe'],
    ['Windows', 'arm64', 'Download Butter Paper for Windows ARM64', 'Butter-Paper-Windows-arm64-Setup.exe'],
    ['Linux', 'x86', 'Download Butter Paper AppImage for Linux x64', 'Butter-Paper-Linux-x64.AppImage'],
    ['Linux', 'arm64', 'Download Butter Paper AppImage for Linux ARM64', 'Butter-Paper-Linux-arm64.AppImage'],
    ['macOS', 'arm64', 'Download Butter Paper DMG for Apple Silicon Mac', 'Butter-Paper-macOS-arm64.dmg'],
    ['macOS', 'x64', 'Download Butter Paper DMG for Intel Mac', 'Butter-Paper-macOS-x64.dmg'],
  ])('recommends the matching %s %s package', async (platform, architecture, label, asset) => {
    const dom = await loadPage({ architecture, platform });
    expect(hero(dom).label).toBe(label);
    expect(hero(dom).href).toContain(asset);
  });

  it('asks unknown platforms to choose a download', async () => {
    const dom = await loadPage();
    expect(hero(dom).label).toBe('Choose your download');
    expect(hero(dom).link.getAttribute('aria-disabled')).toBe('true');
  });

  it('switches stable and beta identity, icon, labels, and asset names together', async () => {
    const dom = await loadPage({ architecture: 'arm64', platform: 'macOS' });
    const document = dom.window.document;
    document.getElementById('channel-beta')!.click();

    expect(document.title).toBe('Download Butter Paper Beta');
    expect(document.getElementById('page-title')!.textContent).toBe('Butter Paper Beta');
    expect(document.getElementById('channel-beta')!.getAttribute('aria-pressed')).toBe('true');
    expect(document.getElementById('app-icon')!.getAttribute('src')).toBe('./assets/icons/beta/icon.png');
    expect(document.getElementById('favicon')!.getAttribute('href')).toBe('./assets/icons/beta/icon.png');
    expect(dom.window.getComputedStyle(document.body).getPropertyValue('--accent').trim()).toBe('#0a5e9d');
    expect(betaIcon).toContain('#1b57f3');
    expect(html).not.toContain('#cc4f47');
    expect(hero(dom).label).toBe('Download Butter Paper Beta DMG for Apple Silicon Mac');
    expect(hero(dom).href).toContain('Butter-Paper-Beta-macOS-arm64.dmg');
    expect(document.getElementById('homebrew-code')!.textContent).toContain('butter-paper@beta');

    document.getElementById('channel-stable')!.click();
    expect(document.title).toBe('Download Butter Paper');
    expect(document.getElementById('channel-stable')!.getAttribute('aria-pressed')).toBe('true');
  });

  it('switches package formats without losing the selected platform or architecture', async () => {
    const dom = await loadPage({ architecture: 'arm64', platform: 'macOS' });
    const document = dom.window.document;
    document.getElementById('format-zip')!.click();
    expect(hero(dom).href).toContain('Butter-Paper-macOS-arm64.zip');

    document.getElementById('platform-linux')!.click();
    document.getElementById('arch-x64')!.click();
    document.getElementById('format-rpm')!.click();
    expect(hero(dom).label).toBe('Download Butter Paper .rpm for Fedora / RHEL x64');
    expect(hero(dom).href).toContain('Butter-Paper-Linux-x64.rpm');
  });

  it('shows unsigned-package context only for Windows and Linux selections', async () => {
    const dom = await loadPage({ architecture: 'x86', platform: 'Windows' });
    const document = dom.window.document;
    expect((document.getElementById('unsigned-package-notice') as HTMLElement).hidden).toBe(false);
    document.getElementById('platform-mac')!.click();
    expect((document.getElementById('unsigned-package-notice') as HTMLElement).hidden).toBe(true);
  });

  it('uses release metadata for stable and beta version and size details', async () => {
    const dom = await loadPage({
      architecture: 'arm64',
      platform: 'macOS',
      releases: {
        stable: {
          assets: [
            { name: 'Butter-Paper-macOS-arm64.dmg', size: 120_000_000 },
            { name: 'Butter-Paper-Beta-macOS-arm64.dmg', size: 121_000_000 },
          ],
          tag_name: 'v1.2.3',
        },
        beta: {
          assets: [{ name: 'Butter-Paper-Beta-macOS-arm64.dmg', size: 122_000_000 }],
          prerelease: true,
          tag_name: 'v1.3.0-beta.1',
        },
      },
    });
    const document = dom.window.document;
    expect(hero(dom).label).toBe('Download Butter Paper DMG for Apple Silicon Mac · 120 MB');
    expect(document.getElementById('download-detail')!.textContent).toBe('v1.2.3 · 120 MB');

    document.getElementById('channel-beta')!.click();
    expect(hero(dom).label).toBe('Download Butter Paper Beta DMG for Apple Silicon Mac · 122 MB');
    expect(hero(dom).href).toContain('/releases/download/v1.3.0-beta.1/');
    expect(document.getElementById('download-detail')!.textContent).toBe('v1.3.0-beta.1 · 122 MB');
  });

  it('copies the channel-aware Homebrew command with the local-file fallback', async () => {
    const dom = await loadPage({ architecture: 'arm64', platform: 'macOS' });
    const document = dom.window.document;
    let command = '';
    document.execCommand = (name) => {
      command = name;
      return true;
    };
    document.getElementById('homebrew-box')!.click();
    await new Promise((resolveTick) => dom.window.setTimeout(resolveTick, 0));
    expect(document.getElementById('homebrew-code')!.textContent).toContain('brew install --cask butter-paper');
    expect(command).toBe('copy');
    expect(document.getElementById('homebrew-copy')!.textContent).toContain('Copied');
  });
});

describe('download page publication workflow', () => {
  it('prepares a manual checksum-sealed bundle without deployment credentials', () => {
    const workflow = readFileSync('.github/workflows/pages.yml', 'utf8');
    expect(workflow).toContain('workflow_dispatch:');
    expect(workflow).toContain('butter-paper-pages-publication-');
    expect(workflow).toContain('SHA256SUMS');
    expect(workflow).toContain('Apply these exact reviewed bytes manually.');
    expect(workflow).not.toMatch(/PAGES_DEPLOY_KEY|git commit|git push/);
  });
});
