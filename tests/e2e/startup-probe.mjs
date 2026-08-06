import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import globalSetup from './global-setup.mjs';
import { firstWindow, launchButterPaper } from './helpers/electron.mjs';

const repoRoot = resolve(import.meta.dirname, '../..');

await globalSetup();

const provenance = JSON.parse(readFileSync(
  resolve(repoRoot, 'test-results/desktop-dev-provenance.json'),
  'utf8',
));
let app;
try {
  app = await launchButterPaper({ theme: 'light' });
  const page = await firstWindow(app);
  const expectedTitle = `Butter Paper Dev · ${provenance.branch}@${provenance.commit.slice(0, 8)}${provenance.dirty ? ' dirty' : ''}`;
  await page.waitForFunction((title) => document.title === title, expectedTitle);
  const [title, metadata] = await Promise.all([
    page.title(),
    page.evaluate(() => window.butterPaper?.application.getMetadata()),
  ]);
  if (title !== expectedTitle
    || metadata?.version !== provenance.version
    || metadata?.commit !== provenance.commit
    || metadata?.branch !== provenance.branch
    || metadata?.checkoutId !== provenance.checkoutId
    || metadata?.statusFingerprint !== provenance.statusFingerprint
    || metadata?.development !== true) {
    throw new Error(`Electron startup provenance mismatch. Expected ${expectedTitle}; received ${title}.`);
  }
  console.log(`Electron startup probe passed: ${title}`);
} finally {
  await app?.close().catch(() => undefined);
}
