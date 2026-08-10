import { describe, expect, it, vi } from 'vitest';
import type { PdfSignatureValidationReport } from '@butter-paper/core';
import type { SignatureDocumentDescriptor } from '../shared/protocol';
import {
  analyzePdfSignatureDocument,
  emptySignatureDocumentAnalysis,
} from './pdfSignatureDocumentAnalysis';

const inputSha256 = 'a'.repeat(64);
const trust = {
  policyId: 'butter-paper-local-explicit-certificates',
  policyVersion: 1,
  configurationSha256: 'b'.repeat(64),
};
const descriptor: SignatureDocumentDescriptor = {
  handle: 'sigdoc_opaque-document',
  inputSha256,
  byteLength: 128,
  signaturePresence: 'signed',
  trust,
  registeredAt: '2026-08-10T00:00:01Z',
  capabilities: {
    createUnsignedCopy: false,
    certificateSign: false,
    certify: false,
    signatureIncrementalWrite: false,
    signedIncrementalEdit: false,
  },
};

const report = {
  schemaVersion: 1,
  inputSha256,
  validationMode: 'offline',
  validationTime: '2026-08-10T00:00:00Z',
  validationTimeProvenance: 'observed-system-utc',
  engineVersion: '0.1.0',
  inventory: { presence: 'signed' },
  signatures: [{ id: 'signature-1' }],
  trust,
  limitations: [],
  issues: [{ code: 'VALIDATION_OFFLINE_INCOMPLETE', severity: 'warning', message: 'Offline only.' }],
} as unknown as PdfSignatureValidationReport;

describe('PDF signature document analysis', () => {
  it('keeps structural uncertainty read-only when the native validator is unavailable', async () => {
    await expect(analyzePdfSignatureDocument('/main-owned/signed.pdf', Buffer.from('%PDF /ByteRange [0 1]'))).resolves.toMatchObject({
      document: null,
      protection: {
        sourceReadOnly: true,
        status: 'potentially-signed',
        detection: 'byte-range-marker',
      },
      validation: { status: 'unavailable', errorCode: 'ENGINE_UNAVAILABLE' },
    });
  });

  it('keeps a document with no detectable signature read-only when validation is unavailable', async () => {
    await expect(analyzePdfSignatureDocument('/main-owned/unknown.pdf', Buffer.from('%PDF-1.7'))).resolves.toMatchObject({
      document: null,
      protection: {
        sourceReadOnly: true,
        status: 'indeterminate',
        detection: 'validation-unavailable',
      },
      validation: { status: 'unavailable', errorCode: 'ENGINE_UNAVAILABLE' },
    });
  });

  it('registers a complete offline report without exposing the source path', async () => {
    const register = vi.fn(async () => descriptor);
    const analysis = await analyzePdfSignatureDocument(
      '/main-owned/signed.pdf',
      Buffer.from('%PDF /ByteRange [0 1]'),
      {
        client: { validateFile: vi.fn(async () => report) },
        registry: { register } as never,
      },
    );

    expect(register).toHaveBeenCalledWith(expect.objectContaining({
      sourcePath: '/main-owned/signed.pdf',
      capabilities: expect.objectContaining({ certificateSign: false, signedIncrementalEdit: false }),
    }));
    expect(JSON.stringify(analysis)).not.toContain('/main-owned/signed.pdf');
    expect(analysis).toMatchObject({
      document: descriptor,
      protection: { sourceReadOnly: true, status: 'signed', detection: 'validation-report' },
      validation: {
        status: 'complete',
        inputSha256,
        validationMode: 'offline',
        signatureCount: 1,
        issueCount: 1,
      },
    });
  });

  it('does not accept a validator that attempts to use network mode', async () => {
    const analysis = await analyzePdfSignatureDocument(
      '/main-owned/unsigned.pdf',
      Buffer.from('%PDF-1.7'),
      {
        client: {
          validateFile: vi.fn(async () => ({ ...report, validationMode: 'online' as const })),
        },
        registry: { register: vi.fn() } as never,
      },
    );

    expect(analysis).toMatchObject({
      protection: { sourceReadOnly: true, status: 'indeterminate', detection: 'validation-unavailable' },
      validation: { status: 'unavailable', errorCode: 'PROTOCOL_ERROR' },
    });
  });

  it('keeps the empty state indeterminate rather than claiming an unsigned result', () => {
    expect(emptySignatureDocumentAnalysis()).toMatchObject({
      protection: { sourceReadOnly: true, status: 'indeterminate' },
      validation: { status: 'unavailable', errorCode: 'ENGINE_UNAVAILABLE' },
    });
  });
});
