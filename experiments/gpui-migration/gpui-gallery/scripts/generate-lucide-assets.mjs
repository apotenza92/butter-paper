import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'node:path';

const iconNames = [
  'arrow-right', 'chart-area', 'check', 'chevron-down', 'circle', 'cloud', 'command',
  'expand', 'file-plus', 'files', 'hand', 'highlighter', 'image', 'message-square', 'minus', 'mouse-pointer-2',
  'move-horizontal', 'pen-line', 'pentagon', 'plus', 'rectangle-vertical', 'rotate-ccw',
  'rotate-cw', 'route', 'ruler', 'ruler-dimension-line', 'scan-line', 'scan-search',
  'shield-x', 'sliders-horizontal', 'spline', 'square', 'type', 'waypoints', 'x',
  'zoom-in', 'zoom-out',
];

const storeRoot = join(homedir(), 'Library/pnpm/store/v10');
const indexRoot = join(storeRoot, 'index');
let packageIndex;
for (const first of await readdir(indexRoot)) {
  const directory = join(indexRoot, first);
  for (const name of await readdir(directory)) {
    if (name.endsWith('-lucide-react@1.8.0.json')) {
      packageIndex = JSON.parse(await readFile(join(directory, name), 'utf8'));
      break;
    }
  }
  if (packageIndex) break;
}
if (!packageIndex) throw new Error('lucide-react 1.8.0 was not found in the pnpm store');

const outputDir = new URL('../assets/icons/', import.meta.url);
await mkdir(outputDir, { recursive: true });
const railOutputDir = new URL('../assets/icons/rail/', import.meta.url);
await mkdir(railOutputDir, { recursive: true });

for (const name of iconNames) {
  const entry = packageIndex.files[`dist/esm/icons/${name}.js`];
  if (!entry) throw new Error(`Lucide icon is missing: ${name}`);
  const digest = Buffer.from(entry.integrity.split('-', 2)[1], 'base64').toString('hex');
  const source = await readFile(join(storeRoot, 'files', digest.slice(0, 2), digest.slice(2)), 'utf8');
  const match = source.match(/const __iconNode = (\[[\s\S]*?\]);/);
  if (!match) throw new Error(`Could not parse Lucide icon: ${name}`);
  const nodes = Function(`"use strict"; return (${match[1]})`)();
  const body = nodes.map(([tag, attributes]) => {
    const serialized = Object.entries(attributes)
      .filter(([key]) => key !== 'key')
      .map(([key, value]) => `${key.replace(/[A-Z]/g, letter => `-${letter.toLowerCase()}`)}="${value}"`)
      .join(' ');
    return `  <${tag} ${serialized}/>`;
  }).join('\n');
  const svg = strokeWidth => `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${strokeWidth}" stroke-linecap="round" stroke-linejoin="round">\n${body}\n</svg>\n`;
  await writeFile(new URL(`${name}.svg`, outputDir), svg(1.5));
  // RailIcons.tsx uses Lucide's absoluteStrokeWidth at 16 px. The equivalent
  // 24-unit SVG stroke is 1.5 * 24 / 16 = 2.25.
  await writeFile(new URL(`${name}.svg`, railOutputDir), svg(2.25));
}

await writeFile(new URL('continuous.svg', outputDir), `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
  <rect width="12" height="20" x="6" y="2" rx="2"/>
  <path d="M8 12h8"/>
</svg>
`);

console.log(`Generated ${iconNames.length} Lucide SVG assets, rail variants, and the Continuous composite in ${outputDir.pathname}`);
