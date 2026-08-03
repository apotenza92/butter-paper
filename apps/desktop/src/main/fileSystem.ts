import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname } from 'node:path';

export async function readBinaryFile(filePath: string): Promise<Uint8Array> {
  const contents = await readFile(filePath);
  return new Uint8Array(contents);
}

export async function writeBinaryFile(filePath: string, bytes: Uint8Array): Promise<void> {
  await mkdir(dirname(filePath), { recursive: true });
  await writeFile(filePath, Buffer.from(bytes));
}
