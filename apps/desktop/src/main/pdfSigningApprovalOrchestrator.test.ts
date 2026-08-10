import { createHash } from 'node:crypto';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import type { SigningApprovalRequest } from '../shared/signingProtocol';
import { PdfSignatureCoreSigningError } from './pdfSignatureCoreSigning';
import { executePdfSigningApproval, type PdfSigningApprovalOrchestratorDependencies } from './pdfSigningApprovalOrchestrator';

const inputSha256 = 'a'.repeat(64);
const certificateSha256 = 'b'.repeat(64);
const trustSha256 = 'c'.repeat(64);
const documentHandle = 'pdfdoc_document';
const targetHandle = 'pdftarget_target';
const identityHandle = '11111111-1111-4111-8111-111111111111';

const request: SigningApprovalRequest = {
  protocolVersion: 1,
  profile: 'PAdES-B-B',
  operation: 'sign',
  documentHandle,
  targetHandle,
  identityHandle,
  certificateSha256,
  digestAlgorithm: 'SHA-256',
  field: {
    mode: 'new',
    name: 'Approval.Signature',
    pageIndex: 2,
    pageRotation: 90,
    rect: { x: 10, y: 20, width: 120, height: 48 },
    lock: { action: 'include', fieldNames: ['Approval.Signature'] },
  },
  appearance: { mode: 'invisible' },
};
const certificationRequest: SigningApprovalRequest = {
  ...request,
  operation: 'certify',
  certificationPermission: 'form-filling-and-signatures',
};

describe('executePdfSigningApproval', () => {
  it('does not consume renderer capabilities while production signing is disabled', async () => {
    const { dependencies } = createDependencies({ signingEnabled: false });
    const result = await executePdfSigningApproval(10, 20, request, dependencies);

    expect(result).toEqual({ outcome: 'failed', operation: 'sign', errorCode: 'CAPABILITY_DISABLED' });
    expect(dependencies.resolveDocument).not.toHaveBeenCalled();
    expect(dependencies.takeSaveTarget).not.toHaveBeenCalled();
    expect(dependencies.withPkcs12Frame).not.toHaveBeenCalled();
  });

  it('rejects a signed or uncertain source before consuming the Save As target', async () => {
    const { dependencies } = createDependencies({
      resolveProtection: () => ({
        sourceReadOnly: true,
        status: 'potentially-signed',
        detection: 'byte-range-marker',
      }),
    });

    const result = await executePdfSigningApproval(10, 20, request, dependencies);

    expect(result.errorCode).toBe('SIGNED_SOURCE_POLICY');
    expect(dependencies.takeSaveTarget).not.toHaveBeenCalled();
    expect(dependencies.resolveSignatureDocument).not.toHaveBeenCalled();
  });

  it('runs the main-owned mutation and returns only an opaque published document handle', async () => {
    const tempRoot = await mkdtemp(join(tmpdir(), 'butter-paper-signing-orchestrator-'));
    try {
      const controller = new AbortController();
      const { dependencies, fakeClient } = createDependencies({
        tempRoot,
        signingEnabled: true,
        signal: controller.signal,
      });
      const result = await executePdfSigningApproval(10, 20, request, dependencies);

      expect(result).toMatchObject({
        outcome: 'completed',
        operation: 'sign',
        inputSha256,
        outputSha256: expect.any(String),
        outputDocumentHandle: 'pdfdoc_published',
        sourcePreserved: true,
        validatedOutput: true,
        profile: 'PAdES-B-B',
      });
      expect(fakeClient.sign).toHaveBeenCalledWith(
        expect.objectContaining({
          field: {
            kind: 'new',
            name: 'Approval.Signature',
            widget: {
              pageIndex: 2,
              x: 10,
              y: 20,
              width: 120,
              height: 48,
              pageRotation: 90,
              coordinateSpace: 'unrotated-pdf-default-user-space',
            },
            lock: { action: 'include', fieldNames: ['Approval.Signature'] },
          },
          appearance: 'invisible',
        }),
        expect.any(Uint8Array),
        undefined,
        { signal: controller.signal },
      );
      expect(fakeClient.postvalidateSignedMutation).toHaveBeenCalledTimes(1);
      expect(fakeClient.postvalidateSignedMutation).toHaveBeenCalledWith(expect.objectContaining({
        expectedCertificateSha256: certificateSha256,
      }), { signal: controller.signal });
      expect(dependencies.takeSaveTarget).toHaveBeenCalledWith(10, targetHandle);
      expect(dependencies.withPkcs12Frame).toHaveBeenCalledWith(identityHandle, 20, expect.any(Function));
    } finally {
      await rm(tempRoot, { recursive: true, force: true });
    }
  });

  it('keeps certification on the same unsigned-source and postvalidation boundary', async () => {
    const tempRoot = await mkdtemp(join(tmpdir(), 'butter-paper-certification-orchestrator-'));
    try {
      const controller = new AbortController();
      const { dependencies, fakeClient } = createDependencies({
        tempRoot,
        signingEnabled: true,
        signal: controller.signal,
      });
      const result = await executePdfSigningApproval(10, 20, certificationRequest, dependencies);

      expect(result).toMatchObject({ outcome: 'completed', operation: 'certify', profile: 'PAdES-B-B' });
      expect(fakeClient.certify).toHaveBeenCalledWith(
        expect.objectContaining({ certificationPermission: 'form-filling-and-signatures' }),
        expect.any(Uint8Array),
        undefined,
        { signal: controller.signal },
      );
      expect(fakeClient.postvalidateSignedMutation).toHaveBeenCalledWith(
        expect.objectContaining({
          expectedOperation: 'certification',
          expectedAppearance: 'invisible',
          expectedCertificationPermission: 'form-filling-and-signatures',
        }),
        { signal: controller.signal },
      );
      expect(fakeClient.sign).not.toHaveBeenCalled();
    } finally {
      await rm(tempRoot, { recursive: true, force: true });
    }
  });

  it('maps an aborted client operation to cancellation instead of success', async () => {
    const tempRoot = await mkdtemp(join(tmpdir(), 'butter-paper-aborted-signing-orchestrator-'));
    try {
      const controller = new AbortController();
      const { dependencies, fakeClient } = createDependencies({
        tempRoot,
        signal: controller.signal,
      });
      fakeClient.sign.mockImplementationOnce(async (...args) => {
        expect(args[3]).toEqual({ signal: controller.signal });
        controller.abort();
        throw new PdfSignatureCoreSigningError('CANCELLED', 'owner was destroyed');
      });

      const result = await executePdfSigningApproval(10, 20, request, dependencies);

      expect(result).toEqual({ outcome: 'failed', operation: 'sign', errorCode: 'CANCELLED' });
      expect(fakeClient.postvalidateSignedMutation).not.toHaveBeenCalled();
    } finally {
      await rm(tempRoot, { recursive: true, force: true });
    }
  });
});

function createDependencies(overrides: Partial<{
  readonly signingEnabled: boolean;
  readonly signal: AbortSignal;
  readonly resolveProtection: PdfSigningApprovalOrchestratorDependencies['resolveProtection'];
  readonly tempRoot: string;
}> = {}): {
  readonly dependencies: PdfSigningApprovalOrchestratorDependencies;
  readonly fakeClient: ReturnType<typeof createFakeClient>;
} {
  const tempRoot = overrides.tempRoot ?? '/tmp/butter-paper-signing-test';
  const fakeClient = createFakeClient();
  const dependencies = {
    signingEnabled: overrides.signingEnabled ?? true,
    signal: overrides.signal,
    currentTrustConfigurationSha256: trustSha256,
    resolveDocument: vi.fn(async () => ({ sourcePath: '/source/unsigned.pdf' })),
    resolveProtection: vi.fn(overrides.resolveProtection ?? (() => ({
      sourceReadOnly: false,
      status: 'unsigned' as const,
      detection: 'validation-report' as const,
    }))),
    resolveSignatureDocumentHandle: vi.fn(() => 'sigdoc_unsigned'),
    resolveSignatureDocument: vi.fn(async () => ({ sourcePath: '/source/unsigned.pdf', inputSha256 })),
    describeIdentity: vi.fn(() => ({
      certificates: [{ sha256Fingerprint: certificateSha256, suitableForSigning: true }],
    })),
    withPkcs12Frame: vi.fn(async (_handle, _owner, consume) => consume(Buffer.from('pfx'))),
    takeSaveTarget: vi.fn(async () => ({ targetPath: '/destination/signed.pdf' })),
    createSigningClient: vi.fn(() => fakeClient),
    createMutation: vi.fn(async (options) => {
      const inputPath = join(tempRoot, 'input.pdf');
      const outputPath = join(tempRoot, 'output.pdf');
      await writeFile(inputPath, Buffer.from('unsigned-pdf'));
      const mutationRequest = {
        inputSnapshotPath: inputPath,
        outputPath,
        expectedInputSha256: inputSha256,
        expectedCertificateSha256: certificateSha256,
        expectedFieldName: options.expectedFieldName,
      };
      const engineResult = await options.mutate(mutationRequest);
      const outputBytes = await import('node:fs/promises').then((fs) => fs.readFile(outputPath));
      const outputSha256 = createHash('sha256').update(outputBytes).digest('hex');
      const postvalidation = await options.postvalidate({ ...mutationRequest, outputSha256 });
      await options.verifyPublished?.(options.targetPath);
      expect(engineResult.outputSha256).toBe(outputSha256);
      return {
        ...postvalidation,
        bytesWritten: outputBytes.byteLength,
        published: true as const,
      };
    }),
    authorizePublishedDocument: vi.fn(async () => 'pdfdoc_published'),
    releaseDocument: vi.fn(),
  } satisfies PdfSigningApprovalOrchestratorDependencies;
  return { dependencies, fakeClient };
}

function createFakeClient() {
  const client = {
    sign: vi.fn(async (
      payload: { outputPath: string; expectedInputSha256: string; field: { kind: string; name: string }; certificateSha256: string; digestAlgorithm: string },
      _pkcs12Frame: Uint8Array,
      _appearanceFrame: Uint8Array | undefined,
      _options: { readonly signal?: AbortSignal } = {},
    ) => (
      fakeEngineForPayload(payload, 'approval')
    )),
    certify: vi.fn(async (
      payload: { outputPath: string; expectedInputSha256: string; field: { kind: string; name: string }; certificateSha256: string; digestAlgorithm: string; certificationPermission: string },
      _pkcs12Frame: Uint8Array,
      _appearanceFrame: Uint8Array | undefined,
      _options: { readonly signal?: AbortSignal } = {},
    ) => (
      fakeEngineForPayload(payload, 'certification', payload.certificationPermission)
    )),
    postvalidateSignedMutation: vi.fn(async (payload: {
      expectedInputSha256: string;
      expectedOutputSha256: string;
      expectedCertificateSha256: string;
      expectedFieldName: string;
    }, _options: { readonly signal?: AbortSignal } = {}) => ({
      inputSha256: payload.expectedInputSha256,
      outputSha256: payload.expectedOutputSha256,
      fieldName: payload.expectedFieldName,
      certificateSha256: payload.expectedCertificateSha256,
      inputPrefixPreserved: true as const,
      addedSignatureCount: 1 as const,
      priorSignaturesPreserved: true as const,
      newSignatureCoversOutputExceptContents: true as const,
      cryptographicallyValid: true as const,
      structurallyReadable: true as const,
      independentProcess: true as const,
      validator: 'pdf-signature-core-v1-validate-plus-main-prefix' as const,
    })),
  };
  return client;
}

async function fakeEngineForPayload(
  payload: { outputPath: string; expectedInputSha256: string; field: { kind: string; name: string }; certificateSha256: string; digestAlgorithm: string },
  kind: 'approval' | 'certification',
  certificationPermission?: string,
) {
  await writeFile(payload.outputPath, Buffer.from('signed-pdf'));
  const outputBytes = await import('node:fs/promises').then((fs) => fs.readFile(payload.outputPath));
  return fakeEngineResponse({
    inputSha256: payload.expectedInputSha256,
    outputSha256: createHash('sha256').update(outputBytes).digest('hex'),
    fieldName: payload.field.name,
    certificateSha256: payload.certificateSha256,
    digestAlgorithm: payload.digestAlgorithm,
    kind,
    certificationPermission: certificationPermission === undefined
      ? null
      : certificationPermission === 'form-filling-and-signatures' ? 2 : 1,
  });
}

function fakeEngineResponse(values: {
  readonly inputSha256: string;
  readonly outputSha256: string;
  readonly fieldName: string;
  readonly certificateSha256: string;
  readonly digestAlgorithm: string;
  readonly kind: 'approval' | 'certification';
  readonly certificationPermission: 1 | 2 | 3 | null;
}) {
  return {
    appendOnly: true,
    certificateSha256: values.certificateSha256,
    digestAlgorithm: values.digestAlgorithm,
    engineVersion: 'test-engine',
    fieldName: values.fieldName,
    inputSha256: values.inputSha256,
    kind: values.kind,
    outputBytes: 256,
    outputSha256: values.outputSha256,
    postcheck: {
      advancedSignatureBindingExact: true,
      appendOnly: true,
      appendedBytes: 128,
      certificateSha256: values.certificateSha256,
      certificationPermission: values.certificationPermission,
      contentsGapExact: true,
      cryptographicIntegrity: 'intact',
      fieldBindingExact: true,
      fieldName: values.fieldName,
      newSignatureByteRange: [0, 1, 2, 3],
      outputSignatureCount: 1,
      priorSignatureCount: 0,
      sourceBytesPreserved: 128,
      wholeRevisionCovered: true,
    },
    profile: 'PAdES-B-B',
    sourcePreserved: true,
    validatedOutput: true,
  };
}
