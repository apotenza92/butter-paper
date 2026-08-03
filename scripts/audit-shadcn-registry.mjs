import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

const desktopDirectory = 'apps/desktop';
const expected = {
  cliVersion: '4.16.1',
  baseUiVersion: '1.6.0',
  style: 'base-nova',
  base: 'base',
  preset: 'b2fA',
  iconLibrary: 'lucide',
  font: 'geist',
  fontDependency: '5.3.0',
};

function runShadcn(arguments_) {
  const packagePath = resolve('node_modules/shadcn/package.json');
  const shadcnPackage = JSON.parse(readFileSync(packagePath, 'utf8'));
  const binPath = resolve(dirname(packagePath), shadcnPackage.bin);
  return execFileSync(
    process.execPath,
    [binPath, ...arguments_],
    {
      cwd: desktopDirectory,
      encoding: 'utf8',
      env: {
        ...process.env,
        FORCE_COLOR: '0',
        NO_COLOR: '1',
      },
      stdio: ['ignore', 'pipe', 'pipe'],
    },
  );
}

const desktopPackage = JSON.parse(readFileSync(`${desktopDirectory}/package.json`, 'utf8'));
const cliVersion = runShadcn(['--version']).trim();
const info = JSON.parse(runShadcn(['info', '--json']));
const contextViolations = [];

if (cliVersion !== expected.cliVersion) {
  contextViolations.push(`shadcn CLI is ${cliVersion}; expected ${expected.cliVersion}`);
}
if (desktopPackage.devDependencies?.shadcn !== expected.cliVersion) {
  contextViolations.push(`desktop shadcn dependency is ${desktopPackage.devDependencies?.shadcn ?? 'missing'}; expected exact ${expected.cliVersion}`);
}
if (desktopPackage.dependencies?.['@base-ui/react'] !== expected.baseUiVersion) {
  contextViolations.push(`desktop Base UI dependency is ${desktopPackage.dependencies?.['@base-ui/react'] ?? 'missing'}; expected exact ${expected.baseUiVersion}`);
}
if (desktopPackage.dependencies?.['@fontsource-variable/geist'] !== expected.fontDependency) {
  contextViolations.push(`desktop Geist dependency is ${desktopPackage.dependencies?.['@fontsource-variable/geist'] ?? 'missing'}; expected exact ${expected.fontDependency}`);
}
if (desktopPackage.dependencies?.['@fontsource/dm-sans'] || desktopPackage.devDependencies?.['@fontsource/dm-sans']) {
  contextViolations.push('desktop must not retain the retired DM Sans dependency');
}
if (info.config?.style !== expected.style || info.config?.base !== expected.base) {
  contextViolations.push(`registry context is ${info.config?.style ?? 'unknown'}/${info.config?.base ?? 'unknown'}; expected ${expected.style}/${expected.base}`);
}
if (info.config?.rsc !== false || info.project?.typescript !== true || info.project?.tailwindVersion !== 'v4') {
  contextViolations.push('registry context must remain non-RSC TypeScript with Tailwind v4');
}
if (info.config?.iconLibrary !== expected.iconLibrary) {
  contextViolations.push(`registry icon library is ${info.config?.iconLibrary ?? 'unknown'}; expected ${expected.iconLibrary}`);
}
if (info.preset?.code !== expected.preset) {
  contextViolations.push(`registry preset is ${info.preset?.code ?? 'unknown'}; expected ${expected.preset}`);
}
if (info.preset?.values?.font !== expected.font) {
  contextViolations.push(`registry font is ${info.preset?.values?.font ?? 'unknown'}; expected ${expected.font}`);
}

if (contextViolations.length > 0) {
  console.error('shadcn project context audit failed:');
  for (const violation of contextViolations) {
    console.error(`- ${violation}`);
  }
  process.exit(1);
}

const components = Array.isArray(info.components) ? info.components : [];
if (components.length === 0) {
  console.error('shadcn project context did not report any installed components.');
  process.exit(1);
}

const drift = [];
for (const component of components) {
  process.stdout.write(`Auditing ${component}... `);
  const componentFile = `src/renderer/src/components/ui/${component}.tsx`;
  const output = runShadcn(['add', component, '--diff', componentFile, '--yes']);
  const fileStatuses = [...output.matchAll(/^\s*[├└]─?\s*(.+?)\s+\((skip|overwrite|create)\)\s*$/gm)]
    .map((match) => ({ filePath: match[1].trim(), status: match[2] }));
  const componentStatus = fileStatuses.find(({ filePath }) => filePath === componentFile);
  const changedFiles = componentStatus && componentStatus.status !== 'skip' ? [componentFile] : [];

  if (!componentStatus || changedFiles.length > 0) {
    drift.push({ component, changedFiles, output: output.trim() });
    console.log('drift');
  } else {
    console.log('clean');
  }
}

if (drift.length > 0) {
  console.error('\nOfficial shadcn registry drift detected:');
  for (const result of drift) {
    console.error(`\n[${result.component}]`);
    if (result.changedFiles.length > 0) {
      for (const filePath of result.changedFiles) {
        console.error(`- ${filePath}`);
      }
    }
    console.error(result.output);
  }
  console.error('\nNo files were written. Refresh reviewed components with the pinned official shadcn CLI.');
  process.exit(1);
}

console.log(`Official shadcn registry audit passed (${components.length} canonical component files, preset ${expected.preset}).`);
