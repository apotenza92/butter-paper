// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import type {
  LoadedDocumentSignatureProtection,
  LoadedDocumentSignatureValidation,
} from '../../../shared/protocol';
import { SignatureStatusBanner } from './SignatureStatusBanner';

const signedProtection: LoadedDocumentSignatureProtection = {
  sourceReadOnly: true,
  status: 'signed',
  detection: 'validation-report',
};

const completeValidation: LoadedDocumentSignatureValidation = {
  status: 'complete',
  inputSha256: 'a'.repeat(64),
  signaturePresence: 'signed',
  validationMode: 'offline',
  validationTime: '2026-08-10T00:00:00Z',
  validationTimeProvenance: 'observed-system-utc',
  trust: {
    policyId: 'offline-empty',
    policyVersion: 1,
    configurationSha256: 'b'.repeat(64),
  },
  signatureCount: 1,
  issueCount: 0,
};

describe('SignatureStatusBanner', () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it('reports offline validation without claiming trust or integrity', () => {
    act(() => root.render(createElement(SignatureStatusBanner, {
      protection: signedProtection,
      validation: completeValidation,
    })));

    expect(host.querySelector('[data-testid="signature-status-banner"]')).toBeTruthy();
    expect(host.textContent).toContain('Signed PDF — document is read-only');
    expect(host.textContent).toContain('Offline validation complete. 1 signature found; 0 reported validation issues.');
    expect(host.textContent).toContain('No trust or integrity conclusion is implied');
  });

  it('reports unavailable validation and keeps the document read-only', () => {
    act(() => root.render(createElement(SignatureStatusBanner, {
      protection: { ...signedProtection, status: 'indeterminate', detection: 'validation-unavailable' },
      validation: {
        status: 'unavailable',
        errorCode: 'ENGINE_UNAVAILABLE',
        message: 'Offline PDF signature validation is unavailable.',
      },
    })));

    expect(host.textContent).toContain('Signature status unavailable — document is read-only');
    expect(host.textContent).toContain('No trust or integrity conclusion is being reported.');
    expect(host.querySelector('[data-testid="signature-status-banner"] [data-slot="badge"]')?.textContent)
      .toBe('Unavailable');
  });

  it('does not add noise for a validated unsigned document', () => {
    act(() => root.render(createElement(SignatureStatusBanner, {
      protection: {
        sourceReadOnly: false,
        status: 'unsigned',
        detection: 'validation-report',
      },
      validation: { ...completeValidation, signaturePresence: 'unsigned', signatureCount: 0 },
    })));

    expect(host.querySelector('[data-testid="signature-status-banner"]')).toBeNull();
  });
});
