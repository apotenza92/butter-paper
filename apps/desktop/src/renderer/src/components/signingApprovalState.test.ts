import {
  buildSigningApprovalRequest,
  createInitialSigningApprovalDraft,
  validateSigningApprovalDraft,
  type SigningApprovalContext,
} from './signingApprovalState';

const context: SigningApprovalContext = {
  documentHandle: 'pdfdoc_test-document',
  targetHandle: 'pdftarget_test-output',
  identity: {
    identityHandle: 'pdfidentity_test-identity',
    certificateSha256: 'a'.repeat(64),
    subject: 'CN=Test signer',
    issuer: 'CN=Test issuer',
    serialNumber: '01',
    validFrom: '2026-01-01T00:00:00Z',
    validTo: '2027-01-01T00:00:00Z',
    keyAlgorithm: 'RSA-2048',
    privateKeyExported: false,
    passwordRemembered: false,
  },
  capabilities: {
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
  },
  sourceIsUnsigned: true,
  existingFieldNames: [],
  newFieldDefaults: {
    name: 'Signature1',
    pageIndex: 0,
    pageRotation: 0,
    rect: { x: 10, y: 10, width: 180, height: 60 },
    lock: null,
  },
  appearance: {
    defaultMode: 'invisible',
  },
};

describe('signing approval state', () => {
  it('starts with the fail-closed certificate capabilities and builds a path-free request', () => {
    const draft = createInitialSigningApprovalDraft(context);
    expect(validateSigningApprovalDraft(draft, context)).toEqual([]);

    const request = buildSigningApprovalRequest(draft, context);
    expect(request).toMatchObject({
      protocolVersion: 1,
      profile: 'PAdES-B-B',
      operation: 'sign',
      documentHandle: context.documentHandle,
      targetHandle: context.targetHandle,
      identityHandle: context.identity.identityHandle,
      certificateSha256: context.identity.certificateSha256,
      appearance: { mode: 'invisible' },
    });
    expect(JSON.stringify(request)).not.toMatch(/password|privateKey|pfx|filePath|path/i);
  });

  it('does not allow certification of a signed source or an unapproved visible appearance', () => {
    const signedContext = { ...context, sourceIsUnsigned: false };
    const draft = {
      ...createInitialSigningApprovalDraft(signedContext),
      operation: 'certify' as const,
      appearanceMode: 'visible' as const,
    };

    expect(validateSigningApprovalDraft(draft, signedContext)).toEqual(expect.arrayContaining([
      'Certification requires an unsigned source document.',
      'A visible appearance is unavailable until a protected appearance asset is provided.',
    ]));
    expect(buildSigningApprovalRequest(draft, signedContext)).toBeNull();
  });
});
