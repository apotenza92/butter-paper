import { extname, isAbsolute, resolve } from 'node:path';

export function resolvePdfPathsFromCommandLine(
  commandLine: readonly string[],
  workingDirectory: string,
  platform: NodeJS.Platform = process.platform,
): string[] {
  const paths: string[] = [];
  const seen = new Set<string>();

  for (const argument of commandLine) {
    if (!argument || argument.startsWith('-') || extname(argument).toLowerCase() !== '.pdf') {
      continue;
    }

    const filePath = isAbsolute(argument) ? resolve(argument) : resolve(workingDirectory, argument);
    const comparisonKey = platform === 'win32' ? filePath.toLowerCase() : filePath;
    if (!seen.has(comparisonKey)) {
      seen.add(comparisonKey);
      paths.push(filePath);
    }
  }

  return paths;
}
