import { enqueuePendingPdfPaths, hasPendingPdfPaths, takePendingPdfPaths } from './pendingPdfPaths';

describe('pending PDF paths', () => {
  afterEach(() => {
    takePendingPdfPaths();
  });

  it('deduplicates queued paths and drains them atomically', () => {
    enqueuePendingPdfPaths(['/tmp/one.pdf', '/tmp/one.pdf', '/tmp/two.pdf']);

    expect(hasPendingPdfPaths()).toBe(true);
    expect(takePendingPdfPaths()).toEqual(['/tmp/one.pdf', '/tmp/two.pdf']);
    expect(hasPendingPdfPaths()).toBe(false);
  });
});
