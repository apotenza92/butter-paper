import type {
  LoadedDocumentSignatureProtection,
  LoadedDocumentSignatureValidation,
  SignatureDocumentDescriptor,
} from '../shared/protocol';
import {
  PdfSignatureCoreClient,
  PdfSignatureCoreError,
  type PdfSignatureCoreClientOptions,
} from './pdfSignatureCore';
import {
  SignatureDocumentRegistry,
  type SignatureDocumentCapabilities,
} from './signatureDocumentRegistry';

const READ_ONLY_CAPABILITIES: SignatureDocumentCapabilities = {
  createUnsignedCopy: false,
  certificateSign: false,
  certify: false,
  signatureIncrementalWrite: false,
  signedIncrementalEdit: false,
};

type SignatureAnalysisErrorCode = 'ENGINE_UNAVAILABLE' | 'PROTOCOL_ERROR' | 'TIMEOUT' | 'CANCELLED';

export interface PdfSignatureDocumentAnalysis {
  readonly document: SignatureDocumentDescriptor | null;
  readonly protection: LoadedDocumentSignatureProtection;
  readonly validation: LoadedDocumentSignatureValidation;
}

export interface PdfSignatureDocumentAnalysisDependencies {
  readonly client?: Pick<PdfSignatureCoreClient, 'validateFile'>;
  readonly registry?: SignatureDocumentRegistry;
}

export function createDefaultPdfSignatureDocumentAnalysisDependencies(
  options: PdfSignatureCoreClientOptions,
  registry: SignatureDocumentRegistry,
): PdfSignatureDocumentAnalysisDependencies {
  return {
    client: new PdfSignatureCoreClient(options),
    registry,
  };
}

/**
 * Main-process-only signature inspection. Structural fallback is deliberately
 * descriptive: it can make a PDF read-only when a ByteRange marker is present,
 * but it never claims cryptographic validity or trust.
 */
export async function analyzePdfSignatureDocument(
  sourcePath: string,
  sourceBytes: Uint8Array,
  dependencies: PdfSignatureDocumentAnalysisDependencies = {},
): Promise<PdfSignatureDocumentAnalysis> {
  const hasStructuralSignature = hasByteRangeMarker(sourceBytes);
  const fallbackProtection: LoadedDocumentSignatureProtection = {
    // An unavailable validator cannot prove that a document is unsigned.
    // Keep the legacy full-rewrite path disabled until validation completes.
    sourceReadOnly: true,
    status: hasStructuralSignature ? 'potentially-signed' : 'indeterminate',
    detection: hasStructuralSignature ? 'byte-range-marker' : 'no-byte-range-marker',
  };

  if (!dependencies.client || !dependencies.registry) {
    return unavailableAnalysis(fallbackProtection, 'ENGINE_UNAVAILABLE');
  }

  try {
    const report = await dependencies.client.validateFile(sourcePath, {
      onlineValidationAuthorized: false,
    });
    if (report.validationMode !== 'offline') {
      throw new PdfSignatureCoreError(
        'PROTOCOL_ERROR',
        'Offline PDF signature validation returned an unexpected network mode.',
      );
    }
    const descriptor = await dependencies.registry.register({
      sourcePath,
      validationReport: report,
      trust: {
        policyId: report.trust.policyId,
        policyVersion: report.trust.policyVersion,
        configurationSha256: report.trust.configurationSha256,
      },
      capabilities: READ_ONLY_CAPABILITIES,
    });
    const status = report.inventory.presence;
    return {
      document: descriptor,
      protection: {
        sourceReadOnly: status !== 'unsigned',
        status,
        detection: 'validation-report',
      },
      validation: {
        status: 'complete',
        inputSha256: report.inputSha256,
        signaturePresence: status,
        validationMode: report.validationMode,
        validationTime: report.validationTime,
        validationTimeProvenance: report.validationTimeProvenance,
        trust: descriptor.trust,
        signatureCount: report.signatures.length,
        issueCount: report.issues.length,
      },
    };
  } catch (error) {
    if (process.env.BP_TEST_MODE === '1') {
      console.error(`PDF signature validation unavailable in test mode: ${describeAnalysisError(error)}`);
    }
    return unavailableAnalysis(fallbackProtection, mapUnavailableError(error));
  }
}

function hasByteRangeMarker(sourceBytes: Uint8Array): boolean {
  return Buffer.from(sourceBytes).toString('latin1').match(/\/ByteRange\s*\[/) !== null;
}

export function emptySignatureDocumentAnalysis(): PdfSignatureDocumentAnalysis {
  return {
    document: null,
    protection: {
      sourceReadOnly: true,
      status: 'indeterminate',
      detection: 'validation-unavailable',
    },
    validation: {
      status: 'unavailable',
      errorCode: 'ENGINE_UNAVAILABLE',
      message: 'Offline PDF signature validation is unavailable.',
    },
  };
}

function unavailableAnalysis(
  protection: LoadedDocumentSignatureProtection,
  errorCode: SignatureAnalysisErrorCode,
): PdfSignatureDocumentAnalysis {
  return {
    document: null,
    protection: {
      ...protection,
      detection: protection.detection === 'byte-range-marker'
        ? 'byte-range-marker'
        : 'validation-unavailable',
    },
    validation: {
      status: 'unavailable',
      errorCode,
      message: errorCode === 'TIMEOUT'
        ? 'Offline PDF signature validation timed out.'
        : errorCode === 'CANCELLED'
          ? 'Offline PDF signature validation was cancelled.'
          : 'Offline PDF signature validation is unavailable.',
    },
  };
}

function mapUnavailableError(error: unknown): SignatureAnalysisErrorCode {
  if (error instanceof PdfSignatureCoreError) {
    if (error.code === 'TIMEOUT') return 'TIMEOUT';
    if (error.code === 'CANCELLED') return 'CANCELLED';
    if (error.code === 'PROTOCOL_ERROR' || error.code === 'ENGINE_ERROR') return 'PROTOCOL_ERROR';
  }
  return 'ENGINE_UNAVAILABLE';
}

function describeAnalysisError(error: unknown): string {
  if (error instanceof PdfSignatureCoreError) {
    if (error.code !== 'LAUNCH_FAILED') return error.code;
    const cause = error.cause;
    const causeCode = cause && typeof cause === 'object' && 'code' in cause && typeof cause.code === 'string'
      ? cause.code
      : null;
    return `${error.code}:${error.message}${causeCode ? `:${causeCode}` : ''}`;
  }
  if (error instanceof Error && error.message.startsWith('Invalid PDF signature core package:')) {
    return error.message.replace(/^Invalid PDF signature core package:\s*/, '').replace(/\.$/, '');
  }
  return error instanceof Error ? error.name : 'UNKNOWN_ERROR';
}
