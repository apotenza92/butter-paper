import { describe, expect, it } from 'vitest';
import {
  applyTabOrder,
  DOCUMENT_TAB_TOOLTIP_SIDE,
  reorderTabIds,
  resolveActiveTabId,
  resolveHorizontalWheelDelta,
} from './DocumentTabBar';
import { formatDocumentTabLabel } from './domain-ui/ClosableDocumentTab';

describe('formatDocumentTabLabel', () => {
  it('removes only a trailing PDF extension from the visible label', () => {
    expect(formatDocumentTabLabel('site-plan.pdf')).toBe('site-plan');
    expect(formatDocumentTabLabel('Drawing.PDF')).toBe('Drawing');
    expect(formatDocumentTabLabel('archive.plan.pdf')).toBe('archive.plan');
    expect(formatDocumentTabLabel('notes.txt')).toBe('notes.txt');
    expect(formatDocumentTabLabel('.pdf')).toBe('.pdf');
  });
});

describe('document tab tooltips', () => {
  it('opens below the tab bar', () => {
    expect(DOCUMENT_TAB_TOOLTIP_SIDE).toBe('bottom');
  });
});

describe('reorderTabIds', () => {
  it('moves a tab to the target position without changing the other order', () => {
    expect(reorderTabIds(['a', 'b', 'c', 'd'], 'd', 'b')).toEqual(['a', 'd', 'b', 'c']);
    expect(reorderTabIds(['a', 'b', 'c', 'd'], 'a', 'c')).toEqual(['b', 'c', 'a', 'd']);
  });

  it('preserves the original array when the move is invalid or redundant', () => {
    const ids = ['a', 'b', 'c'];
    expect(reorderTabIds(ids, 'b', 'b')).toBe(ids);
    expect(reorderTabIds(ids, 'missing', 'b')).toBe(ids);
    expect(reorderTabIds(ids, 'a', 'missing')).toBe(ids);
  });
});

describe('applyTabOrder', () => {
  const tabs = [{ id: 'a', value: 1 }, { id: 'b', value: 2 }, { id: 'c', value: 3 }];

  it('reorders the original tab objects by id', () => {
    const reordered = applyTabOrder(tabs, ['c', 'a', 'b']);
    expect(reordered).toEqual([tabs[2], tabs[0], tabs[1]]);
  });

  it('rejects incomplete, unknown, or duplicate orders', () => {
    expect(applyTabOrder(tabs, ['a', 'b'])).toBe(tabs);
    expect(applyTabOrder(tabs, ['a', 'b', 'missing'])).toBe(tabs);
    expect(applyTabOrder(tabs, ['a', 'a', 'b'])).toBe(tabs);
  });
});

describe('resolveActiveTabId', () => {
  const tabs = [{ id: 'a' }, { id: 'b' }];

  it('preserves a valid active tab', () => {
    expect(resolveActiveTabId(tabs, 'b')).toBe('b');
  });

  it('recovers the first available tab when active state is missing or stale', () => {
    expect(resolveActiveTabId(tabs, null)).toBe('a');
    expect(resolveActiveTabId(tabs, 'missing')).toBe('a');
  });

  it('allows the intentional empty state when no tabs remain', () => {
    expect(resolveActiveTabId([], 'missing')).toBeNull();
  });
});

describe('resolveHorizontalWheelDelta', () => {
  it('uses horizontal mouse or trackpad input when it is dominant', () => {
    expect(resolveHorizontalWheelDelta({ deltaMode: 0, deltaX: 48, deltaY: 8 }, 800)).toBe(48);
  });

  it('maps ordinary vertical wheel input onto horizontal movement', () => {
    expect(resolveHorizontalWheelDelta({ deltaMode: 0, deltaX: 0, deltaY: 72 }, 800)).toBe(72);
  });

  it('normalizes line and page wheel modes', () => {
    expect(resolveHorizontalWheelDelta({ deltaMode: 1, deltaX: 0, deltaY: -3 }, 800)).toBe(-48);
    expect(resolveHorizontalWheelDelta({ deltaMode: 2, deltaX: 0, deltaY: 1 }, 640)).toBe(640);
  });

  it('uses the dominant axis without doubling diagonal input', () => {
    expect(resolveHorizontalWheelDelta({ deltaMode: 0, deltaX: 20, deltaY: -50 }, 800)).toBe(-50);
    expect(resolveHorizontalWheelDelta({ deltaMode: 0, deltaX: 0, deltaY: 0 }, 800)).toBe(0);
  });
});
