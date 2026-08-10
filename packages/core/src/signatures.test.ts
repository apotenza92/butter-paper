import { describe, expect, it } from 'vitest';
import {
  decideSignaturePolicy,
  isPdfSignatureValidationReport,
  SIGNATURE_MUTATION_OPERATIONS,
  SIGNATURE_POLICY_OPERATIONS,
  SIGNATURE_PRESERVATION_OPERATIONS,
  SIGNATURE_READ_ONLY_OPERATIONS,
  signatureDocumentStateFromReport,
  type PdfSignatureValidationReport,
  type SignatureDocumentState,
  type SignatureEngineCapabilities,
  type SignaturePolicyRequest,
} from './signatures.js';

const disabledEngine: SignatureEngineCapabilities = {
  signatureValidation: false,
  createUnsignedCopy: false,
  certificateSign: false,
  certify: false,
  signatureIncrementalWrite: false,
  onlineValidation: false,
  signedIncrementalEdit: false,
};

const absentValidationEvidence = {
  dssPresent: false,
  vriPresent: false,
  structureStatus: 'absent',
  inventoryComplete: true,
  limitExceeded: false,
  certificates: absentEvidenceCollection(),
  ocspResponses: absentEvidenceCollection(),
  crls: absentEvidenceCollection(),
  vriEntryCount: 0,
  vriEntries: [],
} as const;

const signedDocument: SignatureDocumentState = {
  presence: 'signed',
  certificationPermission: 'not-certified',
  modificationPolicyComplete: true,
  currentInputSha256: 'a'.repeat(64),
  validatedInputSha256: 'a'.repeat(64),
  fieldLocks: [],
};

describe('PDF signature document policy', () => {
  it('classifies every policy operation exactly once', () => {
    const classifiedOperations = [
      ...SIGNATURE_READ_ONLY_OPERATIONS,
      ...SIGNATURE_PRESERVATION_OPERATIONS,
      ...SIGNATURE_MUTATION_OPERATIONS,
    ];

    expect(classifiedOperations).toEqual(SIGNATURE_POLICY_OPERATIONS);
    expect(new Set(classifiedOperations).size).toBe(classifiedOperations.length);
  });

  it.each(SIGNATURE_MUTATION_OPERATIONS)(
    'denies %s whenever validation state is indeterminate',
    (operation) => {
      expect(policy({
        document: {
          ...signedDocument,
          presence: 'indeterminate',
          certificationPermission: 'unknown',
          modificationPolicyComplete: false,
          validatedInputSha256: null,
        },
        operation,
        writerMode: 'full-rewrite',
      })).toEqual({
        outcome: 'deny',
        reason: 'SIGNATURE_STATE_INDETERMINATE',
        sourceReadOnly: true,
      });
    },
  );

  it.each(SIGNATURE_MUTATION_OPERATIONS)(
    'denies %s for an unsigned claim without matching validation hashes',
    (operation) => {
      expect(policy({
        document: {
          ...signedDocument,
          presence: 'unsigned',
          currentInputSha256: null,
          validatedInputSha256: null,
        },
        operation,
        writerMode: 'full-rewrite',
      })).toEqual({
        outcome: 'deny',
        reason: 'VALIDATION_STALE',
        sourceReadOnly: true,
      });
    },
  );

  it.each(SIGNATURE_MUTATION_OPERATIONS)(
    'never authorizes %s through the current full-rewrite writer for a signed source',
    (operation) => {
      const result = policy({
        operation,
        writerMode: 'full-rewrite',
        engine: { ...disabledEngine, createUnsignedCopy: true },
      });

      expect(result.outcome).not.toBe('allow');
      expect(result.outcome).not.toBe('allow-new-revision-with-warning');
      expect(result.sourceReadOnly).toBe(true);
    },
  );

  it('allows current full-rewrite editing only for documents proven unsigned', () => {
    expect(policy({
      document: { ...signedDocument, presence: 'unsigned' },
      operation: 'save',
      writerMode: 'full-rewrite',
    })).toEqual({ outcome: 'allow', reason: 'UNSIGNED_DOCUMENT', sourceReadOnly: false });

    expect(policy({ operation: 'save', writerMode: 'full-rewrite' })).toEqual({
      outcome: 'deny',
      reason: 'UNSIGNED_COPY_UNAVAILABLE',
      sourceReadOnly: true,
    });
    expect(policy({ operation: 'autosave', writerMode: 'full-rewrite' }).outcome).toBe('deny');
    expect(policy({ operation: 'close-save', writerMode: 'full-rewrite' }).outcome).toBe('deny');
    expect(policy({ operation: 'change-page-scale', writerMode: 'full-rewrite' }).outcome).toBe('deny');
  });

  it('fails closed when signature state is missing, stale, or transform policy is incomplete', () => {
    expect(policy({
      document: { ...signedDocument, presence: 'indeterminate' },
      operation: 'annotate',
      writerMode: 'full-rewrite',
    })).toMatchObject({ outcome: 'deny', reason: 'SIGNATURE_STATE_INDETERMINATE' });
    expect(policy({
      document: { ...signedDocument, validatedInputSha256: null },
      operation: 'annotate',
      writerMode: 'incremental',
      engine: { ...disabledEngine, signedIncrementalEdit: true },
    })).toMatchObject({ outcome: 'deny', reason: 'VALIDATION_STALE' });
    expect(policy({
      document: { ...signedDocument, currentInputSha256: 'b'.repeat(64) },
      operation: 'annotate',
      writerMode: 'incremental',
      engine: { ...disabledEngine, signedIncrementalEdit: true },
    })).toMatchObject({ outcome: 'deny', reason: 'VALIDATION_STALE' });
    expect(policy({
      document: { ...signedDocument, modificationPolicyComplete: false },
      operation: 'annotate',
      writerMode: 'incremental',
      engine: { ...disabledEngine, signedIncrementalEdit: true },
    })).toMatchObject({ outcome: 'deny', reason: 'CERTIFICATION_PERMISSION_UNKNOWN' });

    const unsignedWithMalformedEvidence: PdfSignatureValidationReport = {
      ...validationReport(),
      inventory: {
        ...validationReport().inventory,
        presence: 'unsigned',
      },
      signatures: [],
      issues: [{
        code: 'MALFORMED_SIGNATURE',
        severity: 'warning',
        message: 'A signature structure could not be parsed.',
      }],
    };
    expect(signatureDocumentStateFromReport(
      unsignedWithMalformedEvidence,
      unsignedWithMalformedEvidence.inputSha256,
    )).toMatchObject({
      presence: 'indeterminate',
      modificationPolicyComplete: false,
    });

    const unsignedWithIncompleteDssInventory: PdfSignatureValidationReport = {
      ...validationReport(),
      inventory: {
        ...validationReport().inventory,
        presence: 'unsigned',
        validationEvidence: {
          ...absentValidationEvidence,
          dssPresent: true,
          vriPresent: false,
          structureStatus: 'indeterminate',
          inventoryComplete: false,
          limitExceeded: true,
          certificates: {
            present: true,
            referenceCount: 4_097,
            embeddedObjectCount: null,
            malformedEntryCount: null,
            inspectionComplete: false,
          },
        },
      },
      signatures: [],
      issues: [],
    };
    expect(signatureDocumentStateFromReport(
      unsignedWithIncompleteDssInventory,
      unsignedWithIncompleteDssInventory.inputSha256,
    )).toMatchObject({ presence: 'indeterminate', modificationPolicyComplete: false });
  });

  it('permits only a byte-identical original copy regardless of validation availability', () => {
    expect(policy({ operation: 'save-original-copy', writerMode: 'byte-copy' })).toEqual({
      outcome: 'allow',
      reason: 'BYTE_IDENTICAL_COPY',
      sourceReadOnly: true,
    });
    expect(policy({ operation: 'save-original-copy', writerMode: 'full-rewrite' }).outcome).toBe('deny');
  });

  it('offers an explicit unsigned copy without ever authorizing mutation of the source', () => {
    const engine = { ...disabledEngine, createUnsignedCopy: true };
    expect(policy({ operation: 'annotate', writerMode: 'full-rewrite', engine })).toEqual({
      outcome: 'make-unsigned-copy',
      reason: 'UNSIGNED_COPY_REQUIRED',
      sourceReadOnly: true,
    });
    expect(policy({ operation: 'create-unsigned-copy', writerMode: 'none', engine })).toEqual({
      outcome: 'make-unsigned-copy',
      reason: 'UNSIGNED_COPY_REQUIRED',
      sourceReadOnly: true,
    });
  });

  it('keeps online validation opt-in even when the engine supports it', () => {
    const engine = { ...disabledEngine, signatureValidation: true, onlineValidation: true };
    expect(policy({ operation: 'validate-offline', writerMode: 'none', engine }).outcome).toBe('allow');
    expect(policy({ operation: 'refresh-validation-online', writerMode: 'none', engine })).toMatchObject({
      outcome: 'deny',
      reason: 'ONLINE_VALIDATION_NOT_AUTHORIZED',
    });
    expect(policy({
      operation: 'refresh-validation-online',
      writerMode: 'none',
      engine,
      onlineValidationAuthorized: true,
    }).outcome).toBe('allow');
  });

  it('requires a proven incremental writer and honors all DocMDP permission levels', () => {
    const engine = { ...disabledEngine, signedIncrementalEdit: true, certificateSign: true };
    expect(policy({ operation: 'annotate', writerMode: 'full-rewrite', engine }).outcome).toBe('deny');
    expect(policy({
      document: { ...signedDocument, presence: 'certified', certificationPermission: 'no-changes' },
      operation: 'annotate',
      writerMode: 'incremental',
      engine,
    })).toMatchObject({ outcome: 'deny', reason: 'CERTIFICATION_FORBIDS_CHANGE' });
    expect(policy({
      document: { ...signedDocument, presence: 'certified', certificationPermission: 'form-filling-and-signatures' },
      operation: 'fill-form-field',
      fieldName: 'Approved',
      writerMode: 'incremental',
      engine,
    }).outcome).toBe('allow-new-revision-with-warning');
    expect(policy({
      document: { ...signedDocument, presence: 'certified', certificationPermission: 'form-filling-and-signatures' },
      operation: 'annotate',
      writerMode: 'incremental',
      engine,
    })).toMatchObject({ outcome: 'deny', reason: 'CERTIFICATION_FORBIDS_OPERATION' });
    expect(policy({
      document: { ...signedDocument, presence: 'certified', certificationPermission: 'form-filling-signatures-and-annotations' },
      operation: 'annotate',
      writerMode: 'incremental',
      engine,
    }).outcome).toBe('allow-new-revision-with-warning');
  });

  it('separates countersigning from broad signed-document editing', () => {
    const signingOnlyEngine = {
      ...disabledEngine,
      certificateSign: true,
      signatureIncrementalWrite: true,
    };

    expect(policy({
      operation: 'sign-existing-field',
      fieldName: 'Approval',
      writerMode: 'incremental',
      engine: signingOnlyEngine,
    })).toEqual({
      outcome: 'allow-new-revision-with-warning',
      reason: 'INCREMENTAL_REVISION',
      sourceReadOnly: true,
    });
    expect(policy({
      operation: 'annotate',
      writerMode: 'incremental',
      engine: signingOnlyEngine,
    })).toEqual({
      outcome: 'deny',
      reason: 'INCREMENTAL_WRITER_REQUIRED',
      sourceReadOnly: true,
    });
    expect(policy({
      operation: 'sign-existing-field',
      fieldName: 'Approval',
      writerMode: 'incremental',
      engine: { ...signingOnlyEngine, signatureIncrementalWrite: false },
    })).toEqual({
      outcome: 'deny',
      reason: 'INCREMENTAL_WRITER_REQUIRED',
      sourceReadOnly: true,
    });
  });

  it('fails closed for missing field names and FieldMDP all/include/exclude locks', () => {
    const engine = { ...disabledEngine, signedIncrementalEdit: true, certificateSign: true };
    expect(policy({ operation: 'fill-form-field', writerMode: 'incremental', engine })).toMatchObject({
      outcome: 'deny', reason: 'FIELD_NAME_REQUIRED',
    });
    for (const fieldLock of [
      { action: 'all' as const, fieldNames: [] },
      { action: 'include' as const, fieldNames: ['Locked'] },
      { action: 'exclude' as const, fieldNames: ['OnlyUnlocked'] },
      { action: 'unknown' as const, fieldNames: [] },
    ]) {
      expect(policy({
        document: { ...signedDocument, fieldLocks: [fieldLock] },
        operation: 'fill-form-field',
        fieldName: 'Locked',
        writerMode: 'incremental',
        engine,
      })).toMatchObject({ outcome: 'deny', reason: 'FIELD_LOCKED' });
    }
  });

  it('derives a fail-closed state unless report hash and current bytes match', () => {
    const report = validationReport();
    expect(signatureDocumentStateFromReport(null, report.inputSha256).presence).toBe('indeterminate');
    expect(signatureDocumentStateFromReport(report, 'b'.repeat(64))).toMatchObject({
      presence: 'indeterminate',
      validatedInputSha256: report.inputSha256,
    });
    expect(signatureDocumentStateFromReport(report, report.inputSha256)).toMatchObject({
      presence: 'signed',
      certificationPermission: 'not-certified',
      modificationPolicyComplete: true,
    });
  });

  it('validates the complete sidecar report boundary and rejects offline network claims', () => {
    const report = validationReport();
    expect(isPdfSignatureValidationReport(report)).toBe(true);
    expect(isPdfSignatureValidationReport({ ...report, inputSha256: 'not-a-hash' })).toBe(false);
    const { validationEvidence: _missingEvidence, ...legacyInventory } = report.inventory;
    expect(isPdfSignatureValidationReport({ ...report, inventory: legacyInventory })).toBe(false);
    for (const validationTime of [
      undefined,
      '2026-08-05T10:00:00+10:00',
      '2026-02-30T00:00:00Z',
      '2026-08-05T00:00:00.000Z',
      '1899-12-31T23:59:59Z',
    ]) {
      expect(isPdfSignatureValidationReport({ ...report, validationTime })).toBe(false);
    }
    expect(isPdfSignatureValidationReport({
      ...report,
      validationTime: '2026-08-05T00:00:00.123456789Z',
    })).toBe(true);
    expect(isPdfSignatureValidationReport({
      ...report,
      validationTimeProvenance: 'renderer-supplied',
    })).toBe(false);
    expect(isPdfSignatureValidationReport({
      ...report,
      trust: { ...report.trust, onlineSourcesUsed: ['https://ocsp.invalid'] },
    })).toBe(false);
    expect(isPdfSignatureValidationReport({
      ...report,
      trust: { ...report.trust, configurationSha256: 'not-a-hash' },
    })).toBe(false);
    expect(isPdfSignatureValidationReport({
      ...report,
      trust: {
        ...report.trust,
        configuredExactCertificateFingerprints: ['b'.repeat(64), 'a'.repeat(64)],
      },
    })).toBe(false);
    const { policyId: _missingPolicyId, ...trustWithoutPolicyId } = report.trust;
    expect(isPdfSignatureValidationReport({ ...report, trust: trustWithoutPolicyId })).toBe(false);
    expect(isPdfSignatureValidationReport({
      ...report,
      inventory: { ...report.inventory, fields: [{ id: 'x', name: 'x', signed: false, widgets: [{ pageIndex: -1, rect: [0, 0, 1, 1] }] }] },
    })).toBe(false);
    expect(isPdfSignatureValidationReport({
      ...report,
      issues: [{
        code: 'SIGNING_EKU_UNSUITABLE',
        severity: 'warning',
        message: 'The certificate key usage is unsuitable for document signing.',
      }],
    })).toBe(true);
    expect(isPdfSignatureValidationReport({
      ...report,
      issues: [{ code: 'UNKNOWN_EKU_CODE', severity: 'warning', message: 'Unknown code.' }],
    })).toBe(false);
  });

  it('requires exact report keys and rejects path-bearing additions at every report level', () => {
    const report = validationReport();
    const signature = report.signatures[0]!;
    const cases = [
      { ...report, sourcePath: '/private/sidecar-secret/source.pdf' },
      { ...report, targetPath: '/private/sidecar-secret/target.pdf' },
      { ...report, canonicalPath: '/private/sidecar-secret/canonical.pdf' },
      { ...report, inventory: { ...report.inventory, sourceFile: '/private/source.pdf' } },
      { ...report, inventory: { ...report.inventory, unexpected: true } },
      { ...report, signatures: [{ ...signature, targetPath: '/private/target.pdf' }] },
      { ...report, trust: { ...report.trust, canonicalFile: '/private/canonical.pdf' } },
    ];

    for (const candidate of cases) {
      expect(isPdfSignatureValidationReport(candidate)).toBe(false);
    }
    expect(isPdfSignatureValidationReport(report)).toBe(true);
    expect(isPdfSignatureValidationReport({
      ...report,
      issues: [{
        code: 'MALFORMED_PDF',
        severity: 'warning',
        message: 'The PDF is malformed.',
        signatureId: 'signature-1',
      }],
    })).toBe(true);
  });

  it('enforces bounded and internally consistent structural DSS/VRI inventory', () => {
    const report = validationReport();
    const collection = {
      present: true,
      referenceCount: 1,
      embeddedObjectCount: 1,
      malformedEntryCount: 0,
      inspectionComplete: true,
    } as const;
    const emptyCollection = absentEvidenceCollection();
    const evidence = {
      dssPresent: true,
      vriPresent: true,
      structureStatus: 'well-formed',
      inventoryComplete: true,
      limitExceeded: false,
      certificates: collection,
      ocspResponses: emptyCollection,
      crls: emptyCollection,
      vriEntryCount: 1,
      vriEntries: [{
        keyReference: 'A'.repeat(40),
        keyReferenceSha256: 'b'.repeat(64),
        keyReferenceFormat: 'sha1-hex',
        structureStatus: 'well-formed',
        certificates: collection,
        ocspResponses: emptyCollection,
        crls: emptyCollection,
      }],
    } as const;
    const withEvidence = (validationEvidence: unknown) => ({
      ...report,
      inventory: { ...report.inventory, validationEvidence },
    });

    expect(isPdfSignatureValidationReport(withEvidence(evidence))).toBe(true);
    expect(isPdfSignatureValidationReport(withEvidence({
      ...evidence,
      certificates: { ...collection, embeddedObjectCount: 0 },
    }))).toBe(false);
    expect(isPdfSignatureValidationReport(withEvidence({
      ...evidence,
      vriEntries: [{ ...evidence.vriEntries[0], keyReference: 'a'.repeat(40) }],
    }))).toBe(false);
    expect(isPdfSignatureValidationReport(withEvidence({
      ...evidence,
      structureStatus: 'malformed',
    }))).toBe(false);
    expect(isPdfSignatureValidationReport(withEvidence({
      ...evidence,
      inventoryComplete: false,
      limitExceeded: true,
      structureStatus: 'indeterminate',
      certificates: {
        present: true,
        referenceCount: 4_097,
        embeddedObjectCount: null,
        malformedEntryCount: null,
        inspectionComplete: false,
      },
    }))).toBe(true);
    expect(isPdfSignatureValidationReport(withEvidence({
      ...evidence,
      certificates: {
        present: true,
        referenceCount: 4_097,
        embeddedObjectCount: 4_097,
        malformedEntryCount: 0,
        inspectionComplete: true,
      },
    }))).toBe(false);
    expect(isPdfSignatureValidationReport(withEvidence({
      ...evidence,
      vriEntryCount: 1_025,
      vriEntries: [],
    }))).toBe(false);
    expect(isPdfSignatureValidationReport(withEvidence({
      dssPresent: true,
      vriPresent: null,
      structureStatus: 'malformed',
      inventoryComplete: true,
      limitExceeded: false,
      certificates: null,
      ocspResponses: null,
      crls: null,
      vriEntryCount: null,
      vriEntries: [],
    }))).toBe(true);
  });

  it('accepts cautious indeterminate evidence freshness for an undependable signature', () => {
    const report = validationReport();
    const hostileReport: PdfSignatureValidationReport = {
      ...report,
      inventory: {
        ...report.inventory,
        presence: 'indeterminate',
        currentRevision: null,
        totalRevisions: null,
        revisionInventoryComplete: false,
        modificationPolicyComplete: false,
      },
      signatures: report.signatures.map((signature) => ({
        ...signature,
        signedRevision: null,
        byteRange: {
          segments: [],
          coveredRevisionEnd: null,
          structurallyValid: false,
        },
        integrity: 'failed',
        identityTrust: 'indeterminate',
        certificateStatus: 'indeterminate',
        signingTime: 'indeterminate',
        modificationStatus: 'unable-to-classify',
        coverage: 'malformed',
        evidenceFreshness: { source: 'indeterminate', producedAt: null, nextUpdateAt: null },
      })),
    };

    expect(isPdfSignatureValidationReport(hostileReport)).toBe(true);
    expect(signatureDocumentStateFromReport(hostileReport, hostileReport.inputSha256)).toMatchObject({
      presence: 'indeterminate',
      modificationPolicyComplete: false,
    });
    expect(policy({
      document: signatureDocumentStateFromReport(hostileReport, hostileReport.inputSha256),
      operation: 'annotate',
      writerMode: 'full-rewrite',
    })).toMatchObject({
      outcome: 'deny',
      reason: 'SIGNATURE_STATE_INDETERMINATE',
      sourceReadOnly: true,
    });
  });

  it('rejects internally contradictory presence, certification, field, and identity claims', () => {
    const report = validationReport();
    expect(isPdfSignatureValidationReport({
      ...report,
      inventory: { ...report.inventory, presence: 'unsigned' },
    })).toBe(false);
    expect(isPdfSignatureValidationReport({
      ...report,
      inventory: {
        ...report.inventory,
        presence: 'unsigned',
        fields: [{ id: 'signed-field', name: 'Signed', signed: true, widgets: [] }],
      },
      signatures: [],
    })).toBe(false);
    expect(isPdfSignatureValidationReport({
      ...report,
      inventory: {
        ...report.inventory,
        presence: 'certified',
        certificationPermission: 'no-changes',
      },
    })).toBe(false);
    expect(isPdfSignatureValidationReport({
      ...report,
      signatures: [...report.signatures, { ...report.signatures[0] }],
    })).toBe(false);
  });
});

function policy(overrides: Partial<SignaturePolicyRequest>) {
  return decideSignaturePolicy({
    document: signedDocument,
    operation: 'view',
    writerMode: 'none',
    engine: disabledEngine,
    ...overrides,
  });
}

function validationReport(): PdfSignatureValidationReport {
  return {
    schemaVersion: 1,
    inputSha256: 'a'.repeat(64),
    validationMode: 'offline',
    validationTime: '2026-08-05T00:00:00Z',
    validationTimeProvenance: 'observed-system-utc',
    engineVersion: 'test',
    inventory: {
      presence: 'signed',
      certificationPermission: 'not-certified',
      currentRevision: 1,
      totalRevisions: 1,
      revisionInventoryComplete: true,
      fields: [],
      modificationPolicyComplete: true,
      validationEvidence: absentValidationEvidence,
    },
    signatures: [{
      id: 'signature-1',
      fieldNames: ['Signature1'],
      kind: 'approval',
      signedRevision: 1,
      byteRange: {
        segments: [[0, 10], [20, 10]],
        coveredRevisionEnd: 30,
        structurallyValid: true,
      },
      transforms: [],
      certificates: [],
      timestamps: [],
      signerClaim: null,
      claimedSigningTime: null,
      integrity: 'intact',
      identityTrust: 'unknown',
      certificateStatus: 'offline',
      signingTime: 'claimed-only',
      modificationStatus: 'none',
      coverage: 'whole-relevant-revision',
      evidenceFreshness: { source: 'none', producedAt: null, nextUpdateAt: null },
      qualification: { padesProfile: 'B-B', claimedCompliant: false, limitations: [] },
      fieldLock: null,
      issues: [],
    }],
    trust: {
      policyId: 'butter-paper-local-explicit-certificates',
      policyVersion: 1,
      configurationSha256: '65621a8373d3e6869d50a8572da7d20ae5c4d7c91a915eeda34493187f071f0e',
      policyName: 'test-offline',
      configuredExactCertificateFingerprints: [],
      onlineSourcesUsed: false,
      limitations: [],
    },
    limitations: [],
    issues: [],
  };
}

function absentEvidenceCollection() {
  return {
    present: false,
    referenceCount: 0,
    embeddedObjectCount: 0,
    malformedEntryCount: 0,
    inspectionComplete: true,
  } as const;
}
