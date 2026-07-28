import { resolvePdfPathsFromCommandLine } from './openPdfPaths';

describe('resolvePdfPathsFromCommandLine', () => {
  it('keeps only PDF paths, resolves relative paths, and removes duplicates', () => {
    expect(resolvePdfPathsFromCommandLine([
      '--inspect',
      'drawing.PDF',
      '/tmp/notes.txt',
      'drawing.PDF',
      '/tmp/detail.pdf',
    ], '/work')).toEqual([
      '/work/drawing.PDF',
      '/tmp/detail.pdf',
    ]);
  });

  it('deduplicates Windows paths without case sensitivity', () => {
    expect(resolvePdfPathsFromCommandLine([
      'C:\\Drawings\\Plan.pdf',
      'c:\\drawings\\PLAN.PDF',
    ], 'C:\\Work', 'win32')).toHaveLength(1);
  });
});
