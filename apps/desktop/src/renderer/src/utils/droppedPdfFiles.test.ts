import { selectDroppedPdfFiles } from './droppedPdfFiles';

describe('selectDroppedPdfFiles', () => {
  it('keeps PDF files regardless of extension casing', () => {
    const files = [
      { name: 'drawing.pdf' },
      { name: 'specification.PDF' },
      { name: 'notes.txt' },
    ] as File[];

    expect(selectDroppedPdfFiles(files)).toEqual([files[0], files[1]]);
  });

  it('does not use a legacy Electron path property', () => {
    const file = { name: 'drawing.pdf', path: '/tmp/drawing.txt' } as File & { path: string };

    expect(selectDroppedPdfFiles([file])).toEqual([file]);
  });
});
