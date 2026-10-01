import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
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

for (const filePath of repositoryFiles) {
  const fileName = basename(filePath);

  const localPlanningDocument = filePath.startsWith('docs/planning/') && extname(filePath).toLowerCase() === '.md';
  if (!localPlanningDocument && (forbiddenStatePath.test(filePath) || forbiddenStateName.test(fileName) || forbiddenPlanName.test(fileName))) {
    violations.push(`${filePath}: keep changing work state in local Markdown under docs/planning/`);
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

const maintainedCommandFiles = ['package.json', ...releaseConfigurationFiles];
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

if (violations.length > 0) {
  console.error('Repository hygiene check failed:');
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  process.exit(1);
}

console.log(`Repository hygiene check passed (${repositoryFiles.length} tracked and untracked repository files inspected).`);
