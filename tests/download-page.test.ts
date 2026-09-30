import { readFileSync } from 'node:fs';
import { JSDOM } from 'jsdom';

const html = readFileSync('docs/index.html', 'utf8');
const readme = readFileSync('README.md', 'utf8');
const productDescription = 'Butter Paper is a free, open-source PDF markup app for macOS, Windows, and Linux. It is a cross-platform alternative to Bluebeam Revu for everyday document review, including architecture, engineering, and construction workflows.';
const readmeProductDescription = readme.split('/>\n\n')[1]?.trim().replaceAll('\n', ' ');

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
  it('keeps the product description aligned with the concise README', async () => {
    const dom = await loadPage();
    expect(readmeProductDescription).toBe(productDescription);
    expect(dom.window.document.querySelector('.subtitle')!.textContent).toBe(readmeProductDescription);
    expect(dom.window.document.querySelector('meta[name="description"]')!.getAttribute('content')).toBe(readmeProductDescription);
    expect(dom.window.document.getElementById('app-icon')!.getAttribute('src')).toBe('./assets/brand/icon.svg');
    expect(dom.window.document.getElementById('favicon')!.getAttribute('href')).toBe('./assets/brand/icon.svg');
    expect(dom.window.document.querySelector('.promise')).toBeNull();
    expect(readme).not.toContain('Clarity you can work on.');
    const repositoryLink = dom.window.document.querySelector('.site-header .repository-link')!;
    expect(repositoryLink.textContent!.trim()).toBe('GitHub repository');
    expect(repositoryLink.getAttribute('href')).toBe('https://github.com/apotenza92/butter-paper');
    expect(readme).toContain('src="assets/butter-paper-icon.png"');
    expect(readme).not.toContain('align="center"');
    expect(readme).not.toContain('<a ');
    expect(readme).not.toContain('## What you can do');
    expect(readme).not.toContain('[Download Butter Paper]');
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
    expect(copy).toContain('PDF markup app');
    expect(copy).toContain('everyday document review');
    expect(copy).toContain('Bluebeam Revu');
    expect(copy).toContain('macOS');
    expect(copy).toContain('Windows');
    expect(copy).toContain('Linux');
    expect(copy).not.toContain(String.fromCodePoint(0x2014));
  });

  it.each([
    ['Windows', 'x86', 'Download Butter Paper for Windows x64', 'Butter-Paper-Windows-x64.zip'],
    ['Windows', 'arm64', 'Download Butter Paper for Windows ARM64', 'Butter-Paper-Windows-arm64.zip'],
    ['Linux', 'x86', 'Download Butter Paper for Linux x64', 'Butter-Paper-Linux-x64.tar.xz'],
    ['Linux', 'arm64', 'Download Butter Paper for Linux ARM64', 'Butter-Paper-Linux-arm64.tar.xz'],
    ['macOS', 'arm64', 'Download Butter Paper for Apple Silicon Mac', 'Butter-Paper-macOS-arm64.zip'],
    ['macOS', 'x64', 'Download Butter Paper for Intel Mac', 'Butter-Paper-macOS-x64.zip'],
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

  it('offers only the native stable packages with no channel or format pickers', async () => {
    const dom = await loadPage({ architecture: 'arm64', platform: 'macOS' });
    const document = dom.window.document;
    expect(document.getElementById('channel-beta')).toBeNull();
    expect(document.getElementById('format-toggle')).toBeNull();

    document.getElementById('platform-linux')!.click();
    document.getElementById('arch-x64')!.click();
    expect(hero(dom).label).toBe('Download Butter Paper for Linux x64');
    expect(hero(dom).href).toContain('Butter-Paper-Linux-x64.tar.xz');
    expect(document.getElementById('homebrew-code')!.textContent).toBe('brew tap apotenza92/tap && brew install --cask butter-paper');
  });

  it('keeps the public copy focused on the product and downloads', async () => {
    const dom = await loadPage({ architecture: 'x86', platform: 'Windows' });
    const publicCopy = `${dom.window.document.body.textContent} ${readme}`;
    expect(publicCopy).not.toMatch(/early-stage|rough edges|unsigned|TUF|provenance|authenticated/i);
  });

  it('uses release metadata for the version and size details', async () => {
    const dom = await loadPage({
      architecture: 'arm64',
      platform: 'macOS',
      releases: {
        stable: {
          assets: [{ name: 'Butter-Paper-macOS-arm64.zip', size: 120_000_000 }],
          tag_name: 'v1.2.3',
        },
      },
    });
    const document = dom.window.document;
    expect(hero(dom).label).toBe('Download Butter Paper for Apple Silicon Mac · 120 MB');
    expect(hero(dom).href).toContain('/releases/download/v1.2.3/Butter-Paper-macOS-arm64.zip');
    expect(document.getElementById('download-detail')!.textContent).toBe('v1.2.3 · 120 MB');
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
  it('deploys the site through the native GitHub Pages environment', () => {
    const workflow = readFileSync('.github/workflows/pages.yml', 'utf8');
    expect(workflow).toContain('workflow_dispatch:');
    expect(workflow).toContain('pages: write');
    expect(workflow).toContain('id-token: write');
    expect(workflow).toContain('name: github-pages');
    expect(workflow).toContain('actions/configure-pages@');
    expect(workflow).toContain('actions/upload-pages-artifact@');
    expect(workflow).toContain('actions/deploy-pages@');
    expect(workflow).toContain('assets/icon-source/butter-paper-origami.svg');
    expect(workflow).toContain('assets/icon-source/butter-paper-origami-beta.svg');
    expect(workflow).not.toMatch(/PAGES_DEPLOY_KEY|git commit|git push|Apply these exact reviewed bytes manually/);
  });
});
