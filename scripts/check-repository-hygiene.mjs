import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { basename, extname } from 'node:path';

const trackedFiles = execFileSync('git', ['ls-files', '-z'], { encoding: 'utf8' })
  .split('\0')
  .filter((filePath) => filePath && existsSync(filePath));

const forbiddenStatePath = /(^|\/)(plans?|subagents?)(\/|$)/i;
const forbiddenStateName = /^(memory|now|worklog|backlog|handoff|roadmap)([-_.].*)?\.md$/i;
const forbiddenPlanName = /(^|[-_.])(plan|roadmap|handoff|worklog)([-_.]|$).*\.md$/i;
const forbiddenGeneratedPath = /(^|\/)(artifacts|test-results|playwright-report|coverage|dist|\.vite|release|target|icon\.iconset)(\/|$)/;
const trackedBuildMetadata = /\.tsbuildinfo$/;
const textExtensions = new Set(['.cjs', '.css', '.html', '.js', '.json', '.md', '.mjs', '.toml', '.ts', '.tsx', '.yaml', '.yml']);

const violations = [];

for (const filePath of trackedFiles) {
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

if (violations.length > 0) {
  console.error('Repository hygiene check failed:');
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  process.exit(1);
}

console.log(`Repository hygiene check passed (${trackedFiles.length} tracked files inspected).`);
