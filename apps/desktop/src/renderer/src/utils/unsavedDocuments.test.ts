import { describe, expect, it, vi } from 'vitest';
import { saveDocumentsInOrder } from './unsavedDocuments';

describe('saveDocumentsInOrder', () => {
  it('saves every document in tab order', async () => {
    const saved: string[] = [];
    await expect(saveDocumentsInOrder(['one', 'two', 'three'], async (document) => {
      saved.push(document);
      return true;
    })).resolves.toBe(true);
    expect(saved).toEqual(['one', 'two', 'three']);
  });

  it('stops when a Save As is cancelled and preserves the remaining documents', async () => {
    const save = vi.fn(async (document: string) => document !== 'two');
    await expect(saveDocumentsInOrder(['one', 'two', 'three'], save)).resolves.toBe(false);
    expect(save).toHaveBeenCalledTimes(2);
    expect(save).not.toHaveBeenCalledWith('three');
  });

  it('stops immediately when saving fails', async () => {
    const failure = new Error('disk full');
    const save = vi.fn(async (document: string) => {
      if (document === 'two') throw failure;
      return true;
    });
    await expect(saveDocumentsInOrder(['one', 'two', 'three'], save)).rejects.toBe(failure);
    expect(save).toHaveBeenCalledTimes(2);
  });
});
