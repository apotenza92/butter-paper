import { dirname, join, resolve } from 'node:path';

export function ancestorFileCandidates(
  applicationPath: string,
  relativePath: string,
  maxParentDepth = 4,
): string[] {
  const candidates: string[] = [];
  let directory = resolve(applicationPath);
  for (let depth = 0; depth <= maxParentDepth; depth += 1) {
    candidates.push(join(directory, relativePath));
    const parent = dirname(directory);
    if (parent === directory) {
      break;
    }
    directory = parent;
  }
  return candidates;
}
