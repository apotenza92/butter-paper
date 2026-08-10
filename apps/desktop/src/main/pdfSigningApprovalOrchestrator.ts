import type {
  LoadedDocumentSignatureProtection,
} from '../shared/protocol';
import type {
  SigningApprovalRequest,
  SigningApprovalResult,
  SigningFieldRequest,
} from '../shared/signingProtocol';
import {
  adaptPdfSignatureCoreSigningResult,
  PdfSignatureCoreSigningError,
  type PdfSignatureCoreCertifyPayload,
  type PdfSignatureCoreSignPayload,
  type PdfSignatureCoreSigningClient,
} from './pdfSignatureCoreSigning';
import {
  createPdfSignedMutation,
  PdfSignedMutationError,
  type CreatePdfSignedMutationOptions,
  type PdfSignedMutationResult,
} from './pdfSignedMutationWorkflow';

interface SigningCertificateDescriptor {
  readonly sha256Fingerprint: string;
  readonly suitableForSigning: boolean;
}

/**
 * The production call chain is deliberately assembled from narrow main-only
 * capabilities. This keeps path resolution, identity bytes, and publication
 * authority outside the renderer while allowing deterministic tests to replace
 * the native engine and publication boundary.
 */
export interface PdfSigningApprovalOrchestratorDependencies {
  readonly signingEnabled: boolean;
  /** Owner teardown cancellation shared by the mutation workflow and sidecar calls. */
  readonly signal?: AbortSignal;
  readonly currentTrustConfigurationSha256: string;
  readonly resolveDocument: (ownerWebContentsId: number, documentHandle: string) => Promise<{
    readonly sourcePath: string;
  }>;
  readonly resolveProtection: (documentHandle: string) => LoadedDocumentSignatureProtection | undefined;
  readonly resolveSignatureDocumentHandle: (documentHandle: string) => string | undefined;
  readonly resolveSignatureDocument: (handle: string, currentTrustConfigurationSha256: string) => Promise<{
    readonly sourcePath: string;
    readonly inputSha256: string;
  }>;
  readonly describeIdentity: (identityHandle: string, ownerWindowId: number) => {
    readonly certificates: readonly SigningCertificateDescriptor[];
  };
  readonly withPkcs12Frame: <T>(
    identityHandle: string,
    ownerWindowId: number,
    consume: (pkcs12Frame: Uint8Array) => Promise<T>,
  ) => Promise<T>;
  readonly takeSaveTarget: (ownerWebContentsId: number, targetHandle: string) => Promise<{
    readonly targetPath: string;
  }>;
  readonly createSigningClient: () => Pick<
    PdfSignatureCoreSigningClient,
    'sign' | 'certify' | 'postvalidateSignedMutation'
  >;
  readonly createMutation?: (
    options: CreatePdfSignedMutationOptions,
  ) => Promise<PdfSignedMutationResult>;
  /** Opens the published path through the main PDF authority and returns its opaque handle. */
  readonly authorizePublishedDocument: (ownerWebContentsId: number, targetPath: string) => Promise<string>;
  readonly releaseDocument?: (ownerWebContentsId: number, documentHandle: string) => void;
  /** Main-owned appearance bytes. A visible request is rejected if no resolver exists. */
  readonly resolveAppearanceFrame?: (appearanceHandle: string) => Promise<Uint8Array>;
}

export async function executePdfSigningApproval(
  ownerWebContentsId: number,
  ownerWindowId: number,
  request: SigningApprovalRequest,
  dependencies: PdfSigningApprovalOrchestratorDependencies,
): Promise<SigningApprovalResult> {
  if (!dependencies.signingEnabled) {
    return disabledResult(request.operation);
  }

  let outputDocumentHandle: string | undefined;
  let appearanceFrame: Uint8Array | undefined;
  try {
    const source = await dependencies.resolveDocument(ownerWebContentsId, request.documentHandle);
    const protection = dependencies.resolveProtection(request.documentHandle);
    if (!protection || protection.sourceReadOnly || protection.status !== 'unsigned') {
      return {
        outcome: 'failed',
        operation: request.operation,
        errorCode: request.operation === 'certify'
          ? 'CERTIFICATION_REQUIRES_UNSIGNED_SOURCE'
          : 'SIGNED_SOURCE_POLICY',
      };
    }

    const signatureDocumentHandle = dependencies.resolveSignatureDocumentHandle(request.documentHandle);
    if (!signatureDocumentHandle) {
      return failed(request.operation, 'ENGINE_UNAVAILABLE');
    }
    const signatureDocument = await dependencies.resolveSignatureDocument(
      signatureDocumentHandle,
      dependencies.currentTrustConfigurationSha256,
    );
    if (signatureDocument.sourcePath !== source.sourcePath) {
      return failed(request.operation, 'SOURCE_CHANGED');
    }

    const identity = dependencies.describeIdentity(request.identityHandle, ownerWindowId);
    const certificate = identity.certificates.find(
      (candidate) => candidate.sha256Fingerprint === request.certificateSha256,
    );
    if (!certificate || !certificate.suitableForSigning) {
      return failed(request.operation, 'UNSUPPORTED_CERTIFICATE');
    }

    if (request.appearance.mode === 'visible') {
      if (!dependencies.resolveAppearanceFrame) return failed(request.operation, 'ENGINE_UNAVAILABLE');
      appearanceFrame = await dependencies.resolveAppearanceFrame(request.appearance.assetHandle);
    }

    // Target handles are one-shot. Do not consume one for a request that failed
    // any source, trust, identity, or appearance preflight above.
    const target = await dependencies.takeSaveTarget(ownerWebContentsId, request.targetHandle);
    const client = dependencies.createSigningClient();
    const mutation = dependencies.createMutation ?? createPdfSignedMutation;

    const result = await dependencies.withPkcs12Frame(
      request.identityHandle,
      ownerWindowId,
      async (pkcs12Frame) => mutation({
        sourcePath: source.sourcePath,
        expectedSourceSha256: signatureDocument.inputSha256,
        expectedCertificateSha256: request.certificateSha256,
        targetPath: target.targetPath,
        expectedFieldName: request.field.name,
        signal: dependencies.signal,
        mutate: async (mutationRequest) => {
          const payload = {
            inputPath: mutationRequest.inputSnapshotPath,
            outputPath: mutationRequest.outputPath,
            expectedInputSha256: mutationRequest.expectedInputSha256,
            certificateSha256: request.certificateSha256,
            digestAlgorithm: request.digestAlgorithm,
            profile: 'PAdES-B-B' as const,
            field: toCoreField(request.field),
            ...(request.reason === undefined ? {} : { reason: request.reason }),
            ...(request.location === undefined ? {} : { location: request.location }),
            ...(request.contact === undefined ? {} : { contact: request.contact }),
            appearance: request.appearance.mode,
          } satisfies PdfSignatureCoreSignPayload;
          const raw = request.operation === 'certify'
            ? await client.certify({
                ...payload,
                certificationPermission: request.certificationPermission,
              } satisfies PdfSignatureCoreCertifyPayload, pkcs12Frame, appearanceFrame, {
                signal: dependencies.signal,
              })
            : await client.sign(payload, pkcs12Frame, appearanceFrame, {
              signal: dependencies.signal,
            });
          return adaptPdfSignatureCoreSigningResult(raw, {
            inputSha256: mutationRequest.expectedInputSha256,
            outputSha256: await sha256File(mutationRequest.outputPath),
            fieldName: mutationRequest.expectedFieldName,
            certificateSha256: request.certificateSha256,
            kind: request.operation === 'certify' ? 'certification' : 'approval',
            digestAlgorithm: request.digestAlgorithm,
          });
        },
        postvalidate: async (postvalidationRequest) => client.postvalidateSignedMutation({
          inputPath: postvalidationRequest.inputSnapshotPath,
          outputPath: postvalidationRequest.outputPath,
          expectedInputSha256: postvalidationRequest.expectedInputSha256,
          expectedOutputSha256: postvalidationRequest.outputSha256,
          expectedCertificateSha256: postvalidationRequest.expectedCertificateSha256,
          expectedFieldName: postvalidationRequest.expectedFieldName,
          expectedOperation: request.operation === 'certify' ? 'certification' : 'approval',
          expectedAppearance: request.appearance.mode,
          ...(request.operation === 'certify'
            ? { expectedCertificationPermission: request.certificationPermission }
            : {}),
        }, {
          signal: dependencies.signal,
        }),
        verifyPublished: async (targetPath) => {
          outputDocumentHandle = await dependencies.authorizePublishedDocument(ownerWebContentsId, targetPath);
        },
      }),
    );

    return {
      outcome: 'completed',
      operation: request.operation,
      outputDocumentHandle,
      inputSha256: result.inputSha256,
      outputSha256: result.outputSha256,
      sourcePreserved: true,
      validatedOutput: true,
      profile: 'PAdES-B-B',
    };
  } catch (error) {
    if (outputDocumentHandle) {
      dependencies.releaseDocument?.(ownerWebContentsId, outputDocumentHandle);
    }
    return mapSigningError(request.operation, error);
  } finally {
    appearanceFrame?.fill(0);
  }
}

function toCoreField(field: SigningFieldRequest): PdfSignatureCoreSignPayload['field'] {
  if (field.mode === 'existing') {
    return { kind: 'existing', name: field.name };
  }
  return {
    kind: 'new',
    name: field.name,
    widget: {
      pageIndex: field.pageIndex,
      x: field.rect.x,
      y: field.rect.y,
      width: field.rect.width,
      height: field.rect.height,
      pageRotation: field.pageRotation,
      coordinateSpace: 'unrotated-pdf-default-user-space',
    },
    lock: field.lock === null ? undefined : field.lock,
  };
}

function disabledResult(operation: SigningApprovalRequest['operation']): SigningApprovalResult {
  return failed(operation, 'CAPABILITY_DISABLED');
}

function failed(
  operation: SigningApprovalRequest['operation'],
  errorCode: NonNullable<SigningApprovalResult['errorCode']>,
): SigningApprovalResult {
  return { outcome: 'failed', operation, errorCode };
}

function mapSigningError(
  operation: SigningApprovalRequest['operation'],
  error: unknown,
): SigningApprovalResult {
  if (error instanceof PdfSignedMutationError) {
    switch (error.code) {
      case 'SOURCE_CHANGED': return failed(operation, 'SOURCE_CHANGED');
      case 'POSTVALIDATION_FAILED': return failed(operation, 'POSTVALIDATION_FAILED');
      case 'MUTATION_FAILED': return mapEngineCause(operation, error.cause);
      case 'PUBLICATION_FAILED':
      case 'TARGET_EXISTS': return failed(operation, 'OUTPUT_PUBLICATION_FAILED');
      case 'CANCELLED': return failed(operation, 'CANCELLED');
      default: return failed(operation, 'INTERNAL_ERROR');
    }
  }
  if (error instanceof PdfSignatureCoreSigningError) {
    if (error.code === 'CANCELLED') return failed(operation, 'CANCELLED');
    if (error.code === 'CAPABILITY_DISABLED') return failed(operation, 'CAPABILITY_DISABLED');
    if (error.code === 'TIMEOUT' || error.code === 'LAUNCH_FAILED' || error.code === 'PROCESS_CRASHED') {
      return failed(operation, 'ENGINE_UNAVAILABLE');
    }
    return failed(operation, 'INTERNAL_ERROR');
  }
  if (isErrorCode(error, 'IDENTITY_CHANGED')) return failed(operation, 'IDENTITY_UNAVAILABLE');
  if (isErrorCode(error, 'HANDLE_EXPIRED') || isErrorCode(error, 'HANDLE_INVALID')) {
    return failed(operation, 'IDENTITY_UNAVAILABLE');
  }
  if (isErrorCode(error, 'TRUST_SNAPSHOT_MISMATCH') || isErrorCode(error, 'STALE_DOCUMENT')) {
    return failed(operation, 'SOURCE_CHANGED');
  }
  return failed(operation, 'INTERNAL_ERROR');
}

function mapEngineCause(
  operation: SigningApprovalRequest['operation'],
  cause: unknown,
): SigningApprovalResult {
  if (!(cause instanceof PdfSignatureCoreSigningError) || cause.code !== 'ENGINE_ERROR') {
    return failed(operation, 'ENGINE_UNAVAILABLE');
  }
  switch (cause.engineCode) {
    case 'PKCS12_UNLOCK_FAILED':
      return failed(operation, 'WRONG_PASSWORD');
    case 'CERTIFICATE_INVALID':
    case 'UNSUPPORTED_SIGNING_KEY':
    case 'UNSUPPORTED_DIGEST_ALGORITHM':
      return failed(operation, 'UNSUPPORTED_CERTIFICATE');
    default:
      return failed(operation, 'ENGINE_UNAVAILABLE');
  }
}

function isErrorCode(error: unknown, code: string): boolean {
  return typeof error === 'object' && error !== null && 'code' in error
    && (error as { code?: unknown }).code === code;
}

async function sha256File(path: string): Promise<string> {
  // The sidecar result is independently bound to the workflow's output after
  // it returns. This digest is only the client-result adapter's required input.
  // The real workflow verifies the output bytes again before accepting it.
  const { createHash } = await import('node:crypto');
  const { readFile } = await import('node:fs/promises');
  return createHash('sha256').update(await readFile(path)).digest('hex');
}
