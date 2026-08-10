import { describe, expect, it } from 'vitest';
import type { SigningApprovalRequest } from '../shared/signingProtocol';
import {
  assertSigningApprovalRequest,
  SigningApprovalBoundaryError,
  signingCapabilityDisabledResult,
  signingIdentitySelectionDisabledResult,
} from './signingApprovalBoundary';

const request: SigningApprovalRequest = {
  protocolVersion: 1,
  profile: 'PAdES-B-B',
  operation: 'sign',
  documentHandle: 'pdfdoc_opaque-document',
  targetHandle: 'pdftarget_opaque-target',
  identityHandle: 'pdfidentity_opaque-identity',
  certificateSha256: 'a'.repeat(64),
  digestAlgorithm: 'SHA-256',
  field: {
    mode: 'new',
    name: 'Signature1',
    pageIndex: 0,
    pageRotation: 0,
    rect: { x: 10, y: 10, width: 180, height: 60 },
    lock: null,
  },
  appearance: { mode: 'invisible' },
};

describe('signing approval boundary', () => {
  it('accepts the closed semantic request and keeps the disabled result truthful', () => {
    expect(() => assertSigningApprovalRequest(request)).not.toThrow();
    expect(signingCapabilityDisabledResult('sign')).toEqual({
      outcome: 'failed',
      operation: 'sign',
      errorCode: 'CAPABILITY_DISABLED',
    });
    expect(signingIdentitySelectionDisabledResult()).toEqual({
      outcome: 'failed',
      errorCode: 'CAPABILITY_DISABLED',
    });
  });

  it('rejects secrets, raw paths, and unknown request keys at the main boundary', () => {
    for (const extra of [
      { password: 'secret' },
      { pfxPath: '/tmp/identity.p12' },
      { privateKey: 'not-allowed' },
      { filePath: '/tmp/input.pdf' },
    ]) {
      expect(() => assertSigningApprovalRequest({ ...request, ...extra })).toThrow(SigningApprovalBoundaryError);
    }
  });

  it('rejects malformed nested fields and accepts certification permission only for certification', () => {
    const newField = request.field.mode === 'new' ? request.field : null;
    expect(newField).not.toBeNull();
    expect(() => assertSigningApprovalRequest({
      ...request,
      field: { ...newField!, rect: { ...newField!.rect, width: 0 } },
    })).toThrow(SigningApprovalBoundaryError);
    expect(() => assertSigningApprovalRequest({
      ...request,
      certificationPermission: 'no-changes',
    })).toThrow(SigningApprovalBoundaryError);
    expect(() => assertSigningApprovalRequest({
      ...request,
      operation: 'certify',
      certificationPermission: 'no-changes',
    })).not.toThrow();
  });
});
