import { describe, expect, it } from 'vitest';
import { isDocumentMutationDisabled } from './signatureMutationPolicy';

describe('isDocumentMutationDisabled', () => {
  it('fails closed when signature analysis is missing', () => {
    expect(isDocumentMutationDisabled(undefined)).toBe(true);
  });

  it('allows mutation only for a validated unsigned source', () => {
    expect(isDocumentMutationDisabled({
      sourceReadOnly: false,
      status: 'unsigned',
      detection: 'validation-report',
    })).toBe(false);
  });

  it.each([
    { sourceReadOnly: true, status: 'unsigned' as const, detection: 'validation-report' as const },
    { sourceReadOnly: true, status: 'signed' as const, detection: 'validation-report' as const },
    { sourceReadOnly: true, status: 'certified' as const, detection: 'validation-report' as const },
    { sourceReadOnly: true, status: 'indeterminate' as const, detection: 'validation-unavailable' as const },
    { sourceReadOnly: false, status: 'signed' as const, detection: 'validation-report' as const },
  ])('keeps $status sources read-only', (protection) => {
    expect(isDocumentMutationDisabled(protection)).toBe(true);
  });
});
