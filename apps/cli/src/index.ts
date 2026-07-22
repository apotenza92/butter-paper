#!/usr/bin/env node
import { Command } from 'commander';
import { inspectPdf } from './inspect.js';
import { formatInspection } from './format.js';

const program = new Command();

program
  .name('butter-paper')
  .description('Butter Paper CLI')
  .version('0.0.1');

program
  .command('inspect')
  .argument('<pdfPath>', 'path to a local PDF file')
  .option('--json', 'emit machine-readable JSON')
  .description('Inspect a local PDF file and print metadata')
  .action(async (pdfPath: string, options: { json?: boolean }) => {
    const inspection = await inspectPdf(pdfPath);

    if (options.json) {
      console.log(JSON.stringify(inspection, null, 2));
      return;
    }

    console.log(formatInspection(inspection));
  });

await program.parseAsync(process.argv);
