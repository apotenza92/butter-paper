import { describe, expect, it } from 'vitest';
import { resolveCadViewEnabled } from './featureFlags';

describe('CAD View feature availability', () => {
  it('keeps CAD View parked by default', () => {
    expect(resolveCadViewEnabled(undefined)).toBe(false);
    expect(resolveCadViewEnabled('0')).toBe(false);
  });

  it('allows CAD View to be enabled explicitly for development', () => {
    expect(resolveCadViewEnabled('1')).toBe(true);
  });
});
