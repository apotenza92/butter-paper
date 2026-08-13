import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { DOCUMENT_OPENING_INDICATOR_DELAY_MS, DocumentOpeningIndicator } from './DocumentViewport';

describe('DocumentOpeningIndicator', () => {
  it('does not interrupt ordinary local opens immediately', () => {
    expect(DOCUMENT_OPENING_INDICATOR_DELAY_MS).toBeGreaterThanOrEqual(500);
  });
  it('shows an accessible loading state that explains online file hydration', () => {
    const markup = renderToStaticMarkup(createElement(DocumentOpeningIndicator));

    expect(markup).toContain('aria-live="polite"');
    expect(markup).toContain('role="status"');
    expect(markup).toContain('Opening PDF');
    expect(markup).toContain('storage provider may need to download it first');
  });

  it('shows the file, logical size, provider, progress, and ETA when available', () => {
    const markup = renderToStaticMarkup(createElement(DocumentOpeningIndicator, {
      progress: {
        fileName: 'Drawing Set.pdf',
        sourceName: 'OneDrive',
        totalBytes: 148 * 1024 * 1024,
        bytesRead: 62 * 1024 * 1024,
        phase: 'reading',
        estimatedSecondsRemaining: 18,
      },
    }));

    expect(markup).toContain('Opening “Drawing Set.pdf” · 148 MB');
    expect(markup).toContain('Downloading from OneDrive · 42% · About 20 seconds remaining');
  });

  it('shows a finishing state after the file has been read', () => {
    const markup = renderToStaticMarkup(createElement(DocumentOpeningIndicator, {
      progress: {
        fileName: 'Drawing Set.pdf',
        sourceName: 'OneDrive',
        totalBytes: 1024,
        bytesRead: 1024,
        phase: 'processing',
        estimatedSecondsRemaining: null,
      },
    }));

    expect(markup).toContain('Finishing…');
  });
});
