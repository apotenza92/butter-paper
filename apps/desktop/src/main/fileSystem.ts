import { readFile } from 'node:fs/promises';

export async function readBinaryFile(filePath: string): Promise<Uint8Array> {
  const contents = await readFile(filePath);
  return new Uint8Array(contents);
}
