import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { basename, extname } from 'node:path';

const repositoryFiles = execFileSync(
  'git',
  ['ls-files', '--cached', '--others', '--exclude-standard', '-z'],
  { encoding: 'utf8' },
)
  .split('\0')
  .filter((filePath) => filePath && existsSync(filePath));

const forbiddenStatePath = /(^|\/)(plans?|subagents?)(\/|$)/i;
const forbiddenStateName = /^(memory|now|worklog|backlog|handoff|roadmap)([-_.].*)?\.md$/i;
const forbiddenPlanName = /(^|[-_.])(plan|roadmap|handoff|worklog)([-_.]|$).*\.md$/i;
const forbiddenGeneratedPath = /(^|\/)(artifacts|test-results|playwright-report|coverage|dist|\.vite|release|target|icon\.iconset)(\/|$)/;
const trackedBuildMetadata = /\.tsbuildinfo$/;
const textExtensions = new Set(['.cjs', '.css', '.html', '.js', '.json', '.md', '.mjs', '.toml', '.ts', '.tsx', '.yaml', '.yml']);
const obsoleteWorkflowPaths = ['.github/workflows/packages.yml'];
const retiredReleaseSecretName = /\b(?:MACOS_CSC_LINK|CSC_LINK|APPLE_ID|APPLE_APP_SPECIFIC_PASSWORD|APPLE_ID_PASSWORD)\b/;

const violations = [];

function collectFiles(directory) {
  if (!existsSync(directory)) {
    return [];
  }

  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const filePath = `${directory}/${entry.name}`;
    return entry.isDirectory() ? collectFiles(filePath) : [filePath];
  });
}

for (const filePath of repositoryFiles) {
  const fileName = basename(filePath);

  if (forbiddenStatePath.test(filePath) || forbiddenStateName.test(fileName) || forbiddenPlanName.test(fileName)) {
    violations.push(`${filePath}: changing work state belongs in GitHub issues or pull requests`);
  }

  if (forbiddenGeneratedPath.test(filePath) || trackedBuildMetadata.test(filePath)) {
    violations.push(`${filePath}: generated output must remain untracked`);
  }

  if (!textExtensions.has(extname(filePath).toLowerCase())) {
    continue;
  }

  const contents = readFileSync(filePath, 'utf8');
  if (/\/Users\/[^/]+\//.test(contents) || /\/home\/[^/]+\//.test(contents)) {
    violations.push(`${filePath}: contains a machine-specific home-directory path`);
  }
}

for (const filePath of obsoleteWorkflowPaths) {
  if (existsSync(filePath)) {
    violations.push(`${filePath}: obsolete unsigned packaging workflow must not return`);
  }
}

const releaseConfigurationFiles = repositoryFiles.filter((filePath) => (
  filePath.startsWith('.github/workflows/')
  || filePath === 'apps/desktop/electron-builder.config.cjs'
  || filePath === 'apps/desktop/package.json'
  || filePath === 'package.json'
));
for (const filePath of releaseConfigurationFiles) {
  const contents = readFileSync(filePath, 'utf8');
  if (retiredReleaseSecretName.test(contents)) {
    violations.push(`${filePath}: references a retired Apple signing or notarization secret name`);
  }
  if (filePath.startsWith('.github/workflows/')) {
    for (const match of contents.matchAll(/^\s*uses:\s*([^\s#]+).*$/gm)) {
      const action = match[1];
      if (!action.startsWith('./') && !/@[a-f0-9]{40}$/.test(action)) {
        violations.push(`${filePath}: third-party action is not pinned to a full commit SHA: ${action}`);
      }
    }
  }
}

const maintainedCommandFiles = ['package.json', 'apps/desktop/package.json', ...releaseConfigurationFiles];
for (const filePath of new Set(maintainedCommandFiles)) {
  const contents = readFileSync(filePath, 'utf8');
  const referencedPaths = [
    ...contents.matchAll(/(?:^|[\s'"`])(scripts\/[A-Za-z0-9_.\/-]+\.(?:mjs|cjs|js|sh|ps1))/gm),
    ...contents.matchAll(/uses:\s+(\.\/[A-Za-z0-9_.\/-]+\.ya?ml)\s*$/gm),
  ].map((match) => match[1]);
  for (const referencedPath of referencedPaths) {
    if (!existsSync(referencedPath)) {
      violations.push(`${filePath}: references missing maintained path ${referencedPath}`);
    }
  }
}

const desktopComponentsPath = 'apps/desktop/components.json';
if (!existsSync(desktopComponentsPath)) {
  violations.push(`${desktopComponentsPath}: maintained desktop frontend must keep its shadcn configuration`);
} else {
  const desktopComponents = JSON.parse(readFileSync(desktopComponentsPath, 'utf8'));
  if (desktopComponents.style !== 'base-rhea') {
    violations.push(`${desktopComponentsPath}: maintained desktop frontend must use the official Base UI Rhea style`);
  }
  if (desktopComponents.iconLibrary !== 'lucide') {
    violations.push(`${desktopComponentsPath}: maintained desktop frontend must preserve Lucide icons`);
  }
}

const desktopPackagePath = 'apps/desktop/package.json';
const desktopPackage = JSON.parse(readFileSync(desktopPackagePath, 'utf8'));
if (!desktopPackage.dependencies?.['@base-ui/react']) {
  violations.push(`${desktopPackagePath}: maintained shadcn components require the Base UI primitive package`);
}

const activeUiFiles = [
  desktopPackagePath,
  ...collectFiles('apps/desktop/src/renderer/src').filter((filePath) => textExtensions.has(extname(filePath).toLowerCase())),
];
const forbiddenUiCompatibility = [
  { pattern: /@radix-ui\//, reason: 'Radix packages are not part of the maintained Base UI frontend' },
  { pattern: /from\s+['"]radix-ui['"]/, reason: 'the unified Radix package is not part of the maintained Base UI frontend' },
  { pattern: /\basChild\b/, reason: 'use Base UI render composition instead of the Radix asChild API' },
  { pattern: /--radix-/, reason: 'Radix CSS variables are not valid Base UI contracts' },
  { pattern: /data-\[state=/, reason: 'use Base UI state attributes instead of Radix data-state selectors' },
  { pattern: /\bbp-(?:control|menu|tooltip)(?:-|\b)/, reason: 'obsolete bespoke standard-control styling must not return' },
];

for (const filePath of activeUiFiles) {
  const contents = readFileSync(filePath, 'utf8');
  for (const { pattern, reason } of forbiddenUiCompatibility) {
    if (pattern.test(contents)) {
      violations.push(`${filePath}: ${reason}`);
    }
  }
}

if (violations.length > 0) {
  console.error('Repository hygiene check failed:');
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  process.exit(1);
}

console.log(`Repository hygiene check passed (${repositoryFiles.length} tracked and untracked repository files inspected).`);
