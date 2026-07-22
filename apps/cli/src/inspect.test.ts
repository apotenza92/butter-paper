import { describe, expect, it } from 'vitest';
import { formatInspection } from './format.js';

describe('formatInspection', () => {
  it('formats page metadata compactly', () => {
    const output = formatInspection({
      path: '/tmp/example.pdf',
      pageCount: 2,
      metadata: {
        title: 'Example',
      },
      pages: [
        { pageNumber: 1, width: 612, height: 792, rotation: 0 },
        { pageNumber: 2, width: 842, height: 595, rotation: 90 },
      ],
    });

    expect(output).toContain('Path: /tmp/example.pdf');
    expect(output).toContain('Title: Example');
    expect(output).toContain('Page 2: 842 x 595, rotation 90');
  });
});
