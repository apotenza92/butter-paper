/**
 * Renderer-safe certificate-signing semantics.
 *
 * This contract intentionally contains opaque capability handles and public
 * certificate metadata only. Passwords, private keys, PFX paths/bytes, raw
 * filesystem paths, and renderer-controlled output paths do not belong here.
 */

export const SIGNING_SEMANTIC_PROTOCOL_VERSION = 1 as const;
export const SIGNING_PROFILE = 'PAdES-B-B' as const;

export type SigningOperation = 'sign' | 'certify';
export type SigningDigestAlgorithm = 'SHA-256' | 'SHA-384' | 'SHA-512';
export type SigningAppearanceMode = 'visible' | 'invisible';
export type CertificationPermission =
  | 'no-changes'
  | 'form-filling-and-signatures'
  | 'form-filling-signatures-and-annotations';

export interface SigningFieldLock {
  readonly action: 'all' | 'include' | 'exclude';
  readonly fieldNames: readonly string[];
}

export interface SigningRectangle {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export type SigningFieldRequest =
  | {
      readonly mode: 'existing';
      readonly name: string;
    }
  | {
      readonly mode: 'new';
      readonly name: string;
      readonly pageIndex: number;
      readonly pageRotation: 0 | 90 | 180 | 270;
      readonly rect: SigningRectangle;
      readonly lock: SigningFieldLock | null;
    };

export type SigningAppearanceRequest =
  | {
      readonly mode: 'invisible';
    }
  | {
      readonly mode: 'visible';
      /** Opaque main-owned appearance asset handle; never a path or raw bytes. */
      readonly assetHandle: string;
    };

interface SigningApprovalBase {
  readonly protocolVersion: typeof SIGNING_SEMANTIC_PROTOCOL_VERSION;
  readonly profile: typeof SIGNING_PROFILE;
  readonly documentHandle: string;
  readonly targetHandle: string;
  /** Opaque main-owned identity handle; never a PFX path or private-key handle. */
  readonly identityHandle: string;
  readonly certificateSha256: string;
  readonly digestAlgorithm: SigningDigestAlgorithm;
  readonly field: SigningFieldRequest;
  readonly appearance: SigningAppearanceRequest;
  readonly reason?: string;
  readonly location?: string;
  readonly contact?: string;
}

export type SigningApprovalRequest =
  | (SigningApprovalBase & {
      readonly operation: 'sign';
    })
  | (SigningApprovalBase & {
      readonly operation: 'certify';
      readonly certificationPermission: CertificationPermission;
    });

export interface SigningIdentitySummary {
  readonly identityHandle: string;
  readonly certificateSha256: string;
  readonly subject: string;
  readonly issuer: string;
  readonly serialNumber: string;
  readonly validFrom: string;
  readonly validTo: string;
  readonly keyAlgorithm: string;
  readonly privateKeyExported: false;
  readonly passwordRemembered: false;
}

export type SigningIdentitySelectionResult =
  | {
      readonly outcome: 'selected';
      readonly identity: SigningIdentitySummary;
    }
  | {
      readonly outcome: 'cancelled';
    }
  | {
      readonly outcome: 'failed';
      readonly errorCode: 'IDENTITY_UNAVAILABLE' | 'ENGINE_UNAVAILABLE' | 'CAPABILITY_DISABLED' | 'INTERNAL_ERROR';
    };

export interface SigningCapabilitySnapshot {
  readonly signatureRead: boolean;
  readonly signatureValidation: boolean;
  readonly certificateSign: boolean;
  readonly certify: boolean;
  readonly onlineValidation: false;
  readonly signedIncrementalEdit: false;
  readonly timestamps: false;
  readonly longTermValidation: false;
  readonly pkcs11: false;
  readonly batchSign: false;
}

export type SigningApprovalOutcome = 'completed' | 'cancelled' | 'failed';

export interface SigningApprovalResult {
  readonly outcome: SigningApprovalOutcome;
  readonly operation: SigningOperation;
  readonly outputDocumentHandle?: string;
  readonly inputSha256?: string;
  readonly outputSha256?: string;
  readonly sourcePreserved?: true;
  readonly validatedOutput?: true;
  readonly profile?: typeof SIGNING_PROFILE;
  readonly errorCode?:
    | 'INVALID_REQUEST'
    | 'IDENTITY_UNAVAILABLE'
    | 'WRONG_PASSWORD'
    | 'UNSUPPORTED_CERTIFICATE'
    | 'SOURCE_CHANGED'
    | 'SIGNED_SOURCE_POLICY'
    | 'CERTIFICATION_REQUIRES_UNSIGNED_SOURCE'
    | 'OUTPUT_PUBLICATION_FAILED'
    | 'POSTVALIDATION_FAILED'
    | 'CANCELLED'
    | 'CAPABILITY_DISABLED'
    | 'ENGINE_UNAVAILABLE'
    | 'INTERNAL_ERROR';
}
