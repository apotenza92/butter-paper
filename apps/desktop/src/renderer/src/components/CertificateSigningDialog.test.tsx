// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SigningCapabilitySnapshot, SigningIdentitySummary } from '../../../shared/signingProtocol';
import { CertificateSigningDialog } from './CertificateSigningDialog';
import type { SigningApprovalContext } from './signingApprovalState';

const identity: SigningIdentitySummary = {
  identityHandle: 'identity-handle',
  certificateSha256: 'a'.repeat(64),
  subject: 'CN=Reviewer',
  issuer: 'CN=Local Test CA',
  serialNumber: '1234',
  validFrom: '2026-01-01T00:00:00Z',
  validTo: '2027-01-01T00:00:00Z',
  keyAlgorithm: 'RSA-2048',
  privateKeyExported: false,
  passwordRemembered: false,
};

const capabilities: SigningCapabilitySnapshot = {
  signatureRead: true,
  signatureValidation: true,
  certificateSign: true,
  certify: true,
  onlineValidation: false,
  signedIncrementalEdit: false,
  timestamps: false,
  longTermValidation: false,
  pkcs11: false,
  batchSign: false,
};

const context: SigningApprovalContext = {
  documentHandle: 'document-handle',
  targetHandle: 'new-output-target',
  identity,
  capabilities,
  sourceIsUnsigned: true,
  existingFieldNames: ['ApprovalField'],
  newFieldDefaults: {
    name: 'Signature 1',
    pageIndex: 0,
    pageRotation: 0,
    rect: { x: 36, y: 36, width: 180, height: 48 },
    lock: null,
  },
  appearance: { defaultMode: 'invisible', visibleAssetHandle: 'appearance-handle' },
};

describe('CertificateSigningDialog', () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    document.body.replaceChildren();
    vi.unstubAllGlobals();
  });

  function renderDialog(overrides: Partial<React.ComponentProps<typeof CertificateSigningDialog>> = {}) {
    const props = {
      ...context,
      open: true,
      onOpenChange: vi.fn(),
      onApprove: vi.fn(),
      ...overrides,
    };
    act(() => root.render(createElement(CertificateSigningDialog, props)));
    return props;
  }

  it('requires final human approval and emits no secret or path fields', () => {
    const props = renderDialog();
    const approve = document.body.querySelector<HTMLButtonElement>('[data-testid="certificate-signing-approve"]');
    expect(approve?.disabled).toBe(false);

    act(() => approve?.click());
    expect(props.onApprove).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain('Confirm that you have reviewed');

    const checkbox = document.body.querySelector<HTMLInputElement>('#certificate-signing-human-approval');
    act(() => checkbox?.click());
    act(() => approve?.click());

    expect(props.onApprove).toHaveBeenCalledTimes(1);
    const request = (props.onApprove as ReturnType<typeof vi.fn>).mock.calls[0]?.[0] as Record<string, unknown>;
    expect(request).toMatchObject({ operation: 'sign', profile: 'PAdES-B-B' });
    expect(JSON.stringify(request)).not.toMatch(/password|pfx|path|private.?key|secret/i);
  });

  it('cancels without emitting an approval request', () => {
    const onCancel = vi.fn();
    const props = renderDialog({ onCancel });
    const cancel = Array.from(document.body.querySelectorAll('button')).find((button) => button.textContent === 'Cancel');

    act(() => cancel?.click());

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
    expect(props.onApprove).not.toHaveBeenCalled();
  });

  it('shows failure without implying a completed signature', () => {
    renderDialog({ failureMessage: 'The signing engine is unavailable.' });

    expect(document.body.querySelector('[data-testid="certificate-signing-failure"]')).toBeTruthy();
    expect(document.body.textContent).toContain('No signed output is being reported.');
    expect(document.body.textContent).toContain('Retry approval request');
    expect(document.body.querySelector('[data-testid="certificate-signing-pending"]')).toBeNull();
  });

  it('shows disabled capability state and keeps unavailable certify explicit', () => {
    const disabledCapabilities = { ...capabilities, certificateSign: false, certify: false };
    renderDialog({ capabilities: disabledCapabilities });

    const approve = document.body.querySelector<HTMLButtonElement>('[data-testid="certificate-signing-approve"]');
    expect(approve?.disabled).toBe(true);
    expect(document.body.textContent).toContain('Certification is unavailable');
    expect(document.body.textContent).toContain('Offline-only signing capability');
  });
});
