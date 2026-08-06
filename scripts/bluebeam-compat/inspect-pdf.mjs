#!/usr/bin/env node
import { writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { inspectAnnotations } from './pdf-inspector.mjs';

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const [input, output] = process.argv.slice(2);
  if (!input) throw new Error('Usage: node inspect-pdf.mjs INPUT.pdf [OUTPUT.json]');
  const inspection = await inspectAnnotations(resolve(input));
  const json = `${JSON.stringify(inspection, null, 2)}\n`;
  if (output) await writeFile(resolve(output), json);
  else process.stdout.write(json);
}
