/**
 * Platform-independent PDF signature inventory, validation, and mutation
 * policy. A validation result deliberately has no aggregate `valid` boolean:
 * byte integrity, identity trust, certificate status, time evidence, and
 * later modifications are independent facts.
 */

export const SIGNATURE_VALIDATION_SCHEMA_VERSION = 1 as const;

export type SignaturePresence = 'unsigned' | 'signed' | 'certified' | 'indeterminate';
export type SignatureKind = 'approval' | 'certification' | 'document-timestamp' | 'unknown';
export type CertificationPermission =
  | 'not-certified'
  | 'no-changes'
  | 'form-filling-and-signatures'
  | 'form-filling-signatures-and-annotations'
  | 'unknown';

export type SignatureIntegrity = 'intact' | 'failed' | 'indeterminate' | 'unsupported';
export type SignatureIdentityTrust =
  | 'trusted'
  | 'explicitly-trusted'
  | 'untrusted'
  | 'unknown'
  | 'indeterminate';
export type SignatureCertificateStatus =
  | 'good'
  | 'revoked'
  | 'expired-at-signing'
  | 'expired-now'
  | 'not-yet-valid'
  | 'unknown'
  | 'offline'
  | 'indeterminate';
export type SignatureSigningTime =
  | 'claimed-only'
  | 'timestamp-verified'
  | 'document-timestamp-verified'
  | 'missing'
  | 'indeterminate';
export type SignatureModificationStatus =
  | 'none'
  | 'permitted'
  | 'prohibited'
  | 'indeterminate'
  | 'unable-to-classify';
export type SignatureCoverage =
  | 'whole-relevant-revision'
  | 'partial-or-ambiguous'
  | 'malformed';
export type SignatureEvidenceSource = 'embedded' | 'cached' | 'online' | 'none' | 'indeterminate';
export type SignatureValidationTimeProvenance =
  | 'observed-system-utc'
  | 'caller-supplied-fixed-reference';

export type SignatureValidationErrorCode =
  | 'MALFORMED_PDF'
  | 'MALFORMED_SIGNATURE'
  | 'INVALID_BYTE_RANGE'
  | 'UNSUPPORTED_SIGNATURE'
  | 'CRYPTOGRAPHIC_FAILURE'
  | 'TRUST_PATH_UNAVAILABLE'
  | 'REVOCATION_STATUS_UNKNOWN'
  | 'SIGNING_EKU_UNSUITABLE'
  | 'TIMESTAMP_INVALID'
  | 'MODIFICATION_PROHIBITED'
  | 'VALIDATION_OFFLINE_INCOMPLETE'
  | 'ENGINE_UNAVAILABLE'
  | 'ENGINE_PROTOCOL_ERROR'
  | 'RESOURCE_LIMIT'
  | 'CANCELLED'
  | 'TIMEOUT'
  | 'INTERNAL_ERROR';

export interface SignatureValidationIssue {
  readonly code: SignatureValidationErrorCode;
  readonly severity: 'warning' | 'error';
  /** Cautious, renderer-safe text. It must not contain paths or secret data. */
  readonly message: string;
  readonly signatureId?: string;
}

export interface SignatureEvidenceFreshness {
  readonly source: SignatureEvidenceSource;
  readonly producedAt: string | null;
  readonly nextUpdateAt: string | null;
}

export interface SignatureQualification {
  readonly padesProfile: 'B-B' | 'B-T' | 'B-LT' | 'B-LTA' | 'legacy' | 'unknown';
  readonly claimedCompliant: boolean;
  readonly limitations: readonly string[];
}

export interface PdfSignatureWidget {
  readonly pageIndex: number;
  readonly rect: readonly [number, number, number, number];
}

export interface PdfSignatureField {
  readonly id: string;
  readonly name: string;
  readonly signed: boolean;
  readonly widgets: readonly PdfSignatureWidget[];
}

export interface PdfFieldLock {
  readonly action: 'all' | 'include' | 'exclude' | 'unknown';
  readonly fieldNames: readonly string[];
}

export interface PdfSignatureSummary {
  readonly id: string;
  readonly fieldNames: readonly string[];
  readonly kind: SignatureKind;
  readonly signedRevision: number | null;
  readonly byteRange: PdfSignatureByteRange;
  readonly transforms: readonly PdfSignatureTransform[];
  readonly certificates: readonly PdfSignatureCertificate[];
  readonly timestamps: readonly PdfSignatureTimestamp[];
  readonly signerClaim: string | null;
  readonly claimedSigningTime: string | null;
  readonly integrity: SignatureIntegrity;
  readonly identityTrust: SignatureIdentityTrust;
  readonly certificateStatus: SignatureCertificateStatus;
  readonly signingTime: SignatureSigningTime;
  readonly modificationStatus: SignatureModificationStatus;
  readonly coverage: SignatureCoverage;
  readonly evidenceFreshness: SignatureEvidenceFreshness;
  readonly qualification: SignatureQualification;
  readonly fieldLock: PdfFieldLock | null;
  readonly issues: readonly SignatureValidationIssue[];
}

export interface PdfSignatureByteRange {
  readonly segments: readonly (readonly [number, number])[];
  readonly coveredRevisionEnd: number | null;
  readonly structurallyValid: boolean;
}

export interface PdfSignatureTransform {
  readonly method: 'DocMDP' | 'FieldMDP' | 'UR3' | 'unknown';
  readonly certificationPermission: CertificationPermission;
  readonly fieldLock: PdfFieldLock | null;
  readonly parsed: boolean;
}

export interface PdfSignatureCertificate {
  readonly subject: string;
  readonly issuer: string;
  readonly serialNumber: string;
  readonly sha256Fingerprint: string;
  readonly validFrom: string;
  readonly validTo: string;
  readonly keyAlgorithm: string;
  readonly signingCertificate: boolean;
}

export interface PdfSignatureTimestamp {
  readonly kind: 'signature-timestamp' | 'document-timestamp' | 'claimed-time' | 'unknown';
  readonly time: string | null;
  readonly verified: boolean;
  readonly tsaClaim: string | null;
}

/** Structural PDF references only; these fields never establish evidence validity. */
export interface PdfValidationEvidenceCollection {
  readonly present: boolean;
  readonly referenceCount: number | null;
  readonly embeddedObjectCount: number | null;
  readonly malformedEntryCount: number | null;
  readonly inspectionComplete: boolean;
}

export interface PdfValidationEvidenceVriEntry {
  /** Bounded decoded PDF-name reference, or null when the full reference exceeds 128 characters. */
  readonly keyReference: string | null;
  /** SHA-256 of the full decoded key reference, not a validity or association claim. */
  readonly keyReferenceSha256: string;
  readonly keyReferenceFormat: 'sha1-hex' | 'other';
  readonly structureStatus: 'well-formed' | 'malformed' | 'indeterminate';
  readonly certificates: PdfValidationEvidenceCollection | null;
  readonly ocspResponses: PdfValidationEvidenceCollection | null;
  readonly crls: PdfValidationEvidenceCollection | null;
}

export interface PdfValidationEvidenceInventory {
  readonly dssPresent: boolean;
  /** Null only when a present Catalog DSS value is not a dictionary. */
  readonly vriPresent: boolean | null;
  readonly structureStatus: 'absent' | 'well-formed' | 'malformed' | 'indeterminate';
  readonly inventoryComplete: boolean;
  readonly limitExceeded: boolean;
  readonly certificates: PdfValidationEvidenceCollection | null;
  readonly ocspResponses: PdfValidationEvidenceCollection | null;
  readonly crls: PdfValidationEvidenceCollection | null;
  readonly vriEntryCount: number | null;
  /** Bounded to the sidecar protocol container limit. */
  readonly vriEntries: readonly PdfValidationEvidenceVriEntry[];
}

export interface PdfSignatureInventory {
  readonly presence: SignaturePresence;
  readonly certificationPermission: CertificationPermission;
  readonly currentRevision: number | null;
  readonly totalRevisions: number | null;
  readonly revisionInventoryComplete: boolean;
  readonly fields: readonly PdfSignatureField[];
  /** True only after transform parameters were parsed without ambiguity. */
  readonly modificationPolicyComplete: boolean;
  /** Structural DSS/VRI facts only; presence is not cryptographic validation. */
  readonly validationEvidence: PdfValidationEvidenceInventory;
}

export interface PdfSignatureTrustContext {
  /** Stable renderer-safe identifier for the exact validation policy. */
  readonly policyId: string;
  readonly policyVersion: number;
  /** Binds this report to the effective trust configuration used for validation. */
  readonly configurationSha256: string;
  readonly policyName: string;
  /** Public fingerprints only; certificate DER remains inside the privileged boundary. */
  readonly configuredExactCertificateFingerprints: readonly string[];
  readonly onlineSourcesUsed: false | readonly string[];
  readonly limitations: readonly string[];
}

export interface PdfSignatureValidationReport {
  readonly schemaVersion: typeof SIGNATURE_VALIDATION_SCHEMA_VERSION;
  readonly inputSha256: string;
  readonly validationMode: 'offline' | 'online';
  /** Canonical UTC instant used for every time-relative result in this report. */
  readonly validationTime: string;
  /** Describes the source of validationTime; it is not proof that a host clock is accurate. */
  readonly validationTimeProvenance: SignatureValidationTimeProvenance;
  readonly engineVersion: string;
  readonly inventory: PdfSignatureInventory;
  readonly signatures: readonly PdfSignatureSummary[];
  readonly trust: PdfSignatureTrustContext;
  readonly limitations: readonly string[];
  readonly issues: readonly SignatureValidationIssue[];
}

export interface SignatureEngineCapabilities {
  readonly signatureValidation: boolean;
  readonly createUnsignedCopy: boolean;
  readonly certificateSign: boolean;
  readonly certify: boolean;
  /** Narrow capability for creating a signature as an append-only PDF revision. */
  readonly signatureIncrementalWrite: boolean;
  readonly onlineValidation: boolean;
  /** Broad capability for non-signature edits to signed PDFs; kept independently gated. */
  readonly signedIncrementalEdit: boolean;
}

export const SIGNATURE_READ_ONLY_OPERATIONS = [
  'view',
  'validate-offline',
  'refresh-validation-online',
  'export-validation-report',
] as const;

export const SIGNATURE_PRESERVATION_OPERATIONS = [
  'save-original-copy',
  'create-unsigned-copy',
] as const;

/**
 * Exhaustive allowlist of operations that can change PDF bytes or the
 * in-memory document later written to PDF. Every new mutation route must be
 * added here so the fail-closed policy matrix exercises it.
 */
export const SIGNATURE_MUTATION_OPERATIONS = [
  'annotate',
  'fill-form-field',
  'change-metadata',
  'add-attachment',
  'insert-page',
  'delete-page',
  'reorder-page',
  'rotate-page',
  'crop-page',
  'change-page-scale',
  'autosave',
  'close-save',
  'save',
  'save-as',
  'add-signature-field',
  'sign-existing-field',
  'certify',
] as const;

export const SIGNATURE_POLICY_OPERATIONS = [
  ...SIGNATURE_READ_ONLY_OPERATIONS,
  ...SIGNATURE_PRESERVATION_OPERATIONS,
  ...SIGNATURE_MUTATION_OPERATIONS,
] as const;

export type SignaturePolicyOperation = (typeof SIGNATURE_POLICY_OPERATIONS)[number];

export type SignatureWriterMode = 'none' | 'byte-copy' | 'full-rewrite' | 'incremental';

export interface SignatureDocumentState {
  readonly presence: SignaturePresence;
  readonly certificationPermission: CertificationPermission;
  readonly modificationPolicyComplete: boolean;
  readonly currentInputSha256: string | null;
  readonly validatedInputSha256: string | null;
  readonly fieldLocks: readonly PdfFieldLock[];
}

export interface SignaturePolicyRequest {
  readonly document: SignatureDocumentState;
  readonly operation: SignaturePolicyOperation;
  readonly writerMode: SignatureWriterMode;
  readonly engine: SignatureEngineCapabilities;
  /** Required for form filling or signing when FieldMDP locks exist. */
  readonly fieldName?: string;
  /** Online validation remains off unless this is explicitly true per action. */
  readonly onlineValidationAuthorized?: boolean;
}

export type SignaturePolicyOutcome =
  | 'allow'
  | 'allow-new-revision-with-warning'
  | 'make-unsigned-copy'
  | 'deny';

export type SignaturePolicyReason =
  | 'READ_ONLY_ACTION'
  | 'UNSIGNED_DOCUMENT'
  | 'BYTE_IDENTICAL_COPY'
  | 'ONLINE_VALIDATION_NOT_AUTHORIZED'
  | 'ONLINE_VALIDATION_UNAVAILABLE'
  | 'VALIDATION_UNAVAILABLE'
  | 'SIGNATURE_STATE_INDETERMINATE'
  | 'VALIDATION_STALE'
  | 'UNSIGNED_COPY_UNAVAILABLE'
  | 'UNSIGNED_COPY_REQUIRED'
  | 'SIGNING_UNAVAILABLE'
  | 'CERTIFICATION_UNAVAILABLE'
  | 'INCREMENTAL_WRITER_REQUIRED'
  | 'CERTIFICATION_FORBIDS_CHANGE'
  | 'CERTIFICATION_PERMISSION_UNKNOWN'
  | 'CERTIFICATION_FORBIDS_OPERATION'
  | 'FIELD_LOCKED'
  | 'FIELD_NAME_REQUIRED'
  | 'INCREMENTAL_REVISION';

export interface SignaturePolicyDecision {
  readonly outcome: SignaturePolicyOutcome;
  readonly reason: SignaturePolicyReason;
  /** Signed/certified/indeterminate source remains immutable when true. */
  readonly sourceReadOnly: boolean;
}

const READ_ONLY_OPERATIONS = new Set<SignaturePolicyOperation>([
  'view',
  'export-validation-report',
]);

const FORM_OR_SIGNATURE_OPERATIONS = new Set<SignaturePolicyOperation>([
  'fill-form-field',
  'sign-existing-field',
]);

/**
 * Central fail-closed document policy. In particular, a full-rewrite writer is
 * never allowed to touch a signed, certified, stale, or indeterminate source.
 */
export function decideSignaturePolicy(request: SignaturePolicyRequest): SignaturePolicyDecision {
  const { document, operation, writerMode, engine } = request;
  const protectedSource = document.presence !== 'unsigned';

  if (READ_ONLY_OPERATIONS.has(operation)) {
    return decision('allow', 'READ_ONLY_ACTION', protectedSource);
  }

  if (operation === 'validate-offline') {
    return engine.signatureValidation
      ? decision('allow', 'READ_ONLY_ACTION', protectedSource)
      : decision('deny', 'VALIDATION_UNAVAILABLE', true);
  }

  if (operation === 'refresh-validation-online') {
    if (request.onlineValidationAuthorized !== true) {
      return decision('deny', 'ONLINE_VALIDATION_NOT_AUTHORIZED', true);
    }
    return engine.onlineValidation
      ? decision('allow', 'READ_ONLY_ACTION', protectedSource)
      : decision('deny', 'ONLINE_VALIDATION_UNAVAILABLE', true);
  }

  if (operation === 'save-original-copy') {
    return writerMode === 'byte-copy'
      ? decision('allow', 'BYTE_IDENTICAL_COPY', protectedSource)
      : decision('deny', 'INCREMENTAL_WRITER_REQUIRED', true);
  }

  if (document.presence === 'indeterminate') {
    return decision('deny', 'SIGNATURE_STATE_INDETERMINATE', true);
  }

  if (
    document.currentInputSha256 === null
    || document.validatedInputSha256 === null
    || document.currentInputSha256 !== document.validatedInputSha256
  ) {
    return decision('deny', 'VALIDATION_STALE', true);
  }

  if (operation === 'create-unsigned-copy') {
    if (document.presence === 'unsigned') {
      return decision('deny', 'UNSIGNED_DOCUMENT', false);
    }
    return engine.createUnsignedCopy
      ? decision('make-unsigned-copy', 'UNSIGNED_COPY_REQUIRED', true)
      : decision('deny', 'UNSIGNED_COPY_UNAVAILABLE', true);
  }

  if (document.presence === 'unsigned') {
    if (operation === 'sign-existing-field') {
      return engine.certificateSign && engine.signatureIncrementalWrite && writerMode === 'incremental'
        ? decision('allow-new-revision-with-warning', 'INCREMENTAL_REVISION', false)
        : decision(
            'deny',
            engine.certificateSign ? 'INCREMENTAL_WRITER_REQUIRED' : 'SIGNING_UNAVAILABLE',
            false,
          );
    }
    if (operation === 'certify') {
      return engine.certify && engine.signatureIncrementalWrite && writerMode === 'incremental'
        ? decision('allow-new-revision-with-warning', 'INCREMENTAL_REVISION', false)
        : decision(
            'deny',
            engine.certify ? 'INCREMENTAL_WRITER_REQUIRED' : 'CERTIFICATION_UNAVAILABLE',
            false,
          );
    }
    return decision('allow', 'UNSIGNED_DOCUMENT', false);
  }

  // Nothing below this point may pass through the existing pdf-lib save path.
  if (writerMode !== 'incremental') {
    return engine.createUnsignedCopy
      ? decision('make-unsigned-copy', 'UNSIGNED_COPY_REQUIRED', true)
      : decision('deny', 'UNSIGNED_COPY_UNAVAILABLE', true);
  }

  if (!document.modificationPolicyComplete) {
    return decision('deny', 'CERTIFICATION_PERMISSION_UNKNOWN', true);
  }

  if (operation === 'certify') {
    return decision('deny', 'CERTIFICATION_FORBIDS_OPERATION', true);
  }
  if (operation === 'add-signature-field') {
    return decision('deny', 'CERTIFICATION_FORBIDS_OPERATION', true);
  }

  if (operation === 'sign-existing-field') {
    if (!engine.certificateSign) {
      return decision('deny', 'SIGNING_UNAVAILABLE', true);
    }
    if (!engine.signatureIncrementalWrite) {
      return decision('deny', 'INCREMENTAL_WRITER_REQUIRED', true);
    }
  } else if (!engine.signedIncrementalEdit) {
    return engine.createUnsignedCopy
      ? decision('make-unsigned-copy', 'UNSIGNED_COPY_REQUIRED', true)
      : decision('deny', 'INCREMENTAL_WRITER_REQUIRED', true);
  }

  if (FORM_OR_SIGNATURE_OPERATIONS.has(operation)) {
    if (!request.fieldName) {
      return decision('deny', 'FIELD_NAME_REQUIRED', true);
    }
    if (isFieldLocked(document.fieldLocks, request.fieldName)) {
      return decision('deny', 'FIELD_LOCKED', true);
    }
  }

  switch (document.certificationPermission) {
    case 'not-certified':
      return decision('allow-new-revision-with-warning', 'INCREMENTAL_REVISION', true);
    case 'no-changes':
      return decision('deny', 'CERTIFICATION_FORBIDS_CHANGE', true);
    case 'form-filling-and-signatures':
      return FORM_OR_SIGNATURE_OPERATIONS.has(operation)
        ? decision('allow-new-revision-with-warning', 'INCREMENTAL_REVISION', true)
        : decision('deny', 'CERTIFICATION_FORBIDS_OPERATION', true);
    case 'form-filling-signatures-and-annotations':
      return FORM_OR_SIGNATURE_OPERATIONS.has(operation) || operation === 'annotate'
        ? decision('allow-new-revision-with-warning', 'INCREMENTAL_REVISION', true)
        : decision('deny', 'CERTIFICATION_FORBIDS_OPERATION', true);
    case 'unknown':
      return decision('deny', 'CERTIFICATION_PERMISSION_UNKNOWN', true);
  }
}

export function signatureDocumentStateFromReport(
  report: PdfSignatureValidationReport | null,
  currentInputSha256: string | null,
): SignatureDocumentState {
  if (report === null || currentInputSha256 === null || report.inputSha256 !== currentInputSha256) {
    return {
      presence: 'indeterminate',
      certificationPermission: 'unknown',
      modificationPolicyComplete: false,
      currentInputSha256,
      validatedInputSha256: report?.inputSha256 ?? null,
      fieldLocks: [],
    };
  }

  // An unsigned result is the only state that can enter the current full-
  // rewrite editor. Require the validator to have completed every structural
  // inventory and reported no issue; a syntactically valid but incomplete or
  // malformed result is not proof that the source is safe to rewrite.
  if (report.inventory.presence === 'unsigned' && (
    !report.inventory.revisionInventoryComplete
    || !report.inventory.modificationPolicyComplete
    || !report.inventory.validationEvidence.inventoryComplete
    || !['absent', 'well-formed'].includes(report.inventory.validationEvidence.structureStatus)
    || report.inventory.currentRevision === null
    || report.inventory.totalRevisions === null
    || report.issues.length > 0
  )) {
    return {
      presence: 'indeterminate',
      certificationPermission: 'unknown',
      modificationPolicyComplete: false,
      currentInputSha256,
      validatedInputSha256: report.inputSha256,
      fieldLocks: [],
    };
  }

  return {
    presence: report.inventory.presence,
    certificationPermission: report.inventory.certificationPermission,
    modificationPolicyComplete: report.inventory.modificationPolicyComplete,
    currentInputSha256,
    validatedInputSha256: report.inputSha256,
    fieldLocks: report.signatures.flatMap((signature) => signature.fieldLock ? [signature.fieldLock] : []),
  };
}

const VALIDATION_REPORT_KEYS = [
  'schemaVersion', 'inputSha256', 'validationMode', 'validationTime',
  'validationTimeProvenance', 'engineVersion', 'inventory', 'signatures',
  'trust', 'limitations', 'issues',
] as const;
const SIGNATURE_INVENTORY_KEYS = [
  'presence', 'certificationPermission', 'currentRevision', 'totalRevisions',
  'revisionInventoryComplete', 'fields', 'modificationPolicyComplete',
  'validationEvidence',
] as const;
const VALIDATION_EVIDENCE_INVENTORY_KEYS = [
  'dssPresent', 'vriPresent', 'structureStatus', 'inventoryComplete',
  'limitExceeded', 'certificates', 'ocspResponses', 'crls', 'vriEntryCount',
  'vriEntries',
] as const;
const VALIDATION_EVIDENCE_COLLECTION_KEYS = [
  'present', 'referenceCount', 'embeddedObjectCount', 'malformedEntryCount',
  'inspectionComplete',
] as const;
const VALIDATION_EVIDENCE_VRI_ENTRY_KEYS = [
  'keyReference', 'keyReferenceSha256', 'keyReferenceFormat', 'structureStatus',
  'certificates', 'ocspResponses', 'crls',
] as const;
const SIGNATURE_FIELD_KEYS = ['id', 'name', 'signed', 'widgets'] as const;
const SIGNATURE_WIDGET_KEYS = ['pageIndex', 'rect'] as const;
const SIGNATURE_SUMMARY_KEYS = [
  'id', 'fieldNames', 'kind', 'signedRevision', 'byteRange', 'transforms',
  'certificates', 'timestamps', 'signerClaim', 'claimedSigningTime', 'integrity',
  'identityTrust', 'certificateStatus', 'signingTime', 'modificationStatus',
  'coverage', 'evidenceFreshness', 'qualification', 'fieldLock', 'issues',
] as const;
const BYTE_RANGE_KEYS = ['segments', 'coveredRevisionEnd', 'structurallyValid'] as const;
const TRANSFORM_KEYS = ['method', 'certificationPermission', 'fieldLock', 'parsed'] as const;
const CERTIFICATE_KEYS = [
  'subject', 'issuer', 'serialNumber', 'sha256Fingerprint', 'validFrom', 'validTo',
  'keyAlgorithm', 'signingCertificate',
] as const;
const TIMESTAMP_KEYS = ['kind', 'time', 'verified', 'tsaClaim'] as const;
const EVIDENCE_FRESHNESS_KEYS = ['source', 'producedAt', 'nextUpdateAt'] as const;
const QUALIFICATION_KEYS = ['padesProfile', 'claimedCompliant', 'limitations'] as const;
const FIELD_LOCK_KEYS = ['action', 'fieldNames'] as const;
const TRUST_CONTEXT_KEYS = [
  'policyId', 'policyVersion', 'configurationSha256', 'policyName',
  'configuredExactCertificateFingerprints', 'onlineSourcesUsed', 'limitations',
] as const;
const VALIDATION_ISSUE_KEYS = ['code', 'severity', 'message'] as const;

/** Runtime boundary guard for reports received from the privileged sidecar. */
export function isPdfSignatureValidationReport(value: unknown): value is PdfSignatureValidationReport {
  if (!isRecord(value)
    || !hasExactKeys(value, VALIDATION_REPORT_KEYS)
    || value.schemaVersion !== SIGNATURE_VALIDATION_SCHEMA_VERSION
    || !isSha256(value.inputSha256)
    || !isOneOf(value.validationMode, ['offline', 'online'])
    || !isCanonicalSignatureValidationTime(value.validationTime)
    || !isOneOf(value.validationTimeProvenance, [
      'observed-system-utc',
      'caller-supplied-fixed-reference',
    ])
    || !isString(value.engineVersion)
    || !isSignatureInventory(value.inventory)
    || !isArrayOf(value.signatures, isSignatureSummary)
    || !isTrustContext(value.trust)
    || !isStringArray(value.limitations)
    || !isArrayOf(value.issues, isValidationIssue)) {
    return false;
  }
  return (value.validationMode !== 'offline' || value.trust.onlineSourcesUsed === false)
    && isValidationReportInternallyConsistent(value as unknown as PdfSignatureValidationReport);
}

export function isCanonicalSignatureValidationTime(value: unknown): value is string {
  if (typeof value !== 'string') return false;
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.(\d{3}|\d{6}|\d{9}))?Z$/.exec(value);
  if (!match) return false;
  const [, yearText, monthText, dayText, hourText, minuteText, secondText, fraction] = match;
  const year = Number(yearText);
  const month = Number(monthText);
  const day = Number(dayText);
  const hour = Number(hourText);
  const minute = Number(minuteText);
  const second = Number(secondText);
  if (year < 1900
    || fraction === '000'
    || (fraction?.length === 6 && fraction.endsWith('000'))
    || (fraction?.length === 9 && fraction.endsWith('000'))) return false;
  const milliseconds = Number((fraction ?? '').slice(0, 3).padEnd(3, '0'));
  const parsed = new Date(Date.UTC(year, month - 1, day, hour, minute, second, milliseconds));
  return Number.isFinite(parsed.getTime())
    && parsed.getUTCFullYear() === year
    && parsed.getUTCMonth() === month - 1
    && parsed.getUTCDate() === day
    && parsed.getUTCHours() === hour
    && parsed.getUTCMinutes() === minute
    && parsed.getUTCSeconds() === second;
}

function isValidationReportInternallyConsistent(report: PdfSignatureValidationReport): boolean {
  const signatureIds = report.signatures.map((signature) => signature.id);
  const fieldIds = report.inventory.fields.map((field) => field.id);
  if (new Set(signatureIds).size !== signatureIds.length
    || new Set(fieldIds).size !== fieldIds.length) {
    return false;
  }

  switch (report.inventory.presence) {
    case 'unsigned':
      return report.signatures.length === 0
        && report.inventory.fields.every((field) => !field.signed)
        && report.inventory.certificationPermission === 'not-certified';
    case 'signed':
      return report.signatures.length > 0
        && report.signatures.every((signature) => signature.kind !== 'certification')
        && report.inventory.certificationPermission === 'not-certified';
    case 'certified':
      return report.signatures.length > 0
        && report.signatures.some((signature) => signature.kind === 'certification')
        && report.inventory.certificationPermission !== 'not-certified';
    case 'indeterminate':
      return true;
  }
}

function isSignatureInventory(value: unknown): value is PdfSignatureInventory {
  return isRecord(value)
    && hasExactKeys(value, SIGNATURE_INVENTORY_KEYS)
    && isOneOf(value.presence, ['unsigned', 'signed', 'certified', 'indeterminate'])
    && isCertificationPermission(value.certificationPermission)
    && isNullableNonnegativeInteger(value.currentRevision)
    && isNullableNonnegativeInteger(value.totalRevisions)
    && typeof value.revisionInventoryComplete === 'boolean'
    && isArrayOf(value.fields, isSignatureField)
    && typeof value.modificationPolicyComplete === 'boolean'
    && isValidationEvidenceInventory(value.validationEvidence);
}

function isValidationEvidenceInventory(value: unknown): value is PdfValidationEvidenceInventory {
  if (!isRecord(value)
    || !hasExactKeys(value, VALIDATION_EVIDENCE_INVENTORY_KEYS)
    || typeof value.dssPresent !== 'boolean'
    || !(typeof value.vriPresent === 'boolean' || value.vriPresent === null)
    || !isOneOf(value.structureStatus, ['absent', 'well-formed', 'malformed', 'indeterminate'])
    || typeof value.inventoryComplete !== 'boolean'
    || typeof value.limitExceeded !== 'boolean'
    || !(value.certificates === null || isValidationEvidenceCollection(value.certificates))
    || !(value.ocspResponses === null || isValidationEvidenceCollection(value.ocspResponses))
    || !(value.crls === null || isValidationEvidenceCollection(value.crls))
    || !isNullableNonnegativeInteger(value.vriEntryCount)
    || !isArrayOf(value.vriEntries, isValidationEvidenceVriEntry)
    || value.vriEntries.length > 1_024
    || value.limitExceeded === value.inventoryComplete) {
    return false;
  }

  const collections = [value.certificates, value.ocspResponses, value.crls];
  if (!value.dssPresent) {
    return value.vriPresent === false
      && value.structureStatus === 'absent'
      && value.inventoryComplete
      && !value.limitExceeded
      && value.vriEntryCount === 0
      && value.vriEntries.length === 0
      && collections.every(isAbsentValidationEvidenceCollection);
  }

  if (collections.every((collection) => collection === null)) {
    return value.vriPresent === null
      && value.structureStatus === 'malformed'
      && value.inventoryComplete
      && !value.limitExceeded
      && value.vriEntryCount === null
      && value.vriEntries.length === 0;
  }
  if (collections.some((collection) => collection === null) || value.vriPresent === null) return false;

  if (value.vriPresent === false) {
    if (value.vriEntryCount !== 0 || value.vriEntries.length !== 0) return false;
  } else if (value.vriEntryCount === null) {
    if (value.vriEntries.length !== 0 || value.structureStatus !== 'malformed') return false;
  } else if (value.vriEntryCount > 1_024) {
    if (!value.limitExceeded || value.vriEntries.length !== 0) return false;
  } else if (value.vriEntries.length !== value.vriEntryCount) {
    return false;
  }

  const concreteCollections = collections as PdfValidationEvidenceCollection[];
  const malformedObserved = concreteCollections.some(validationEvidenceCollectionMalformed)
    || value.vriEntryCount === null
    || value.vriEntries.some((entry) => entry.structureStatus === 'malformed');
  const inspectedReferenceCount = [
    ...concreteCollections,
    ...value.vriEntries.flatMap((entry) => [entry.certificates, entry.ocspResponses, entry.crls]),
  ].reduce((total, collection) => (
    total + (collection?.inspectionComplete ? collection.referenceCount ?? 0 : 0)
  ), 0);
  if (inspectedReferenceCount > 4_096) return false;

  if (value.structureStatus === 'malformed') return malformedObserved;
  if (malformedObserved) return false;
  return value.structureStatus === (value.inventoryComplete ? 'well-formed' : 'indeterminate');
}

function isValidationEvidenceCollection(value: unknown): value is PdfValidationEvidenceCollection {
  if (!isRecord(value)
    || !hasExactKeys(value, VALIDATION_EVIDENCE_COLLECTION_KEYS)
    || typeof value.present !== 'boolean'
    || !isNullableNonnegativeInteger(value.referenceCount)
    || !isNullableNonnegativeInteger(value.embeddedObjectCount)
    || !isNullableNonnegativeInteger(value.malformedEntryCount)
    || typeof value.inspectionComplete !== 'boolean') return false;
  if (!value.present) return isAbsentValidationEvidenceCollection(value);
  if (value.referenceCount === null) {
    return value.inspectionComplete
      && value.embeddedObjectCount === null
      && value.malformedEntryCount === 1;
  }
  if (!value.inspectionComplete) {
    return value.embeddedObjectCount === null && value.malformedEntryCount === null;
  }
  return value.embeddedObjectCount !== null
    && value.malformedEntryCount !== null
    && value.embeddedObjectCount + value.malformedEntryCount === value.referenceCount;
}

function isAbsentValidationEvidenceCollection(value: unknown): value is PdfValidationEvidenceCollection {
  return isRecord(value)
    && value.present === false
    && value.referenceCount === 0
    && value.embeddedObjectCount === 0
    && value.malformedEntryCount === 0
    && value.inspectionComplete === true;
}

function validationEvidenceCollectionMalformed(collection: PdfValidationEvidenceCollection): boolean {
  return collection.malformedEntryCount !== null && collection.malformedEntryCount > 0;
}

function isValidationEvidenceVriEntry(value: unknown): value is PdfValidationEvidenceVriEntry {
  if (!isRecord(value)
    || !hasExactKeys(value, VALIDATION_EVIDENCE_VRI_ENTRY_KEYS)
    || !(value.keyReference === null
      || (typeof value.keyReference === 'string' && value.keyReference.length <= 128))
    || !isSha256(value.keyReferenceSha256)
    || !isOneOf(value.keyReferenceFormat, ['sha1-hex', 'other'])
    || !isOneOf(value.structureStatus, ['well-formed', 'malformed', 'indeterminate'])
    || !(value.certificates === null || isValidationEvidenceCollection(value.certificates))
    || !(value.ocspResponses === null || isValidationEvidenceCollection(value.ocspResponses))
    || !(value.crls === null || isValidationEvidenceCollection(value.crls))) return false;

  const collections = [value.certificates, value.ocspResponses, value.crls];
  if (collections.every((collection) => collection === null)) {
    return value.structureStatus === 'malformed';
  }
  if (collections.some((collection) => collection === null)) return false;
  const concreteCollections = collections as PdfValidationEvidenceCollection[];
  const malformed = value.keyReferenceFormat === 'other'
    || concreteCollections.some(validationEvidenceCollectionMalformed);
  const incomplete = concreteCollections.some((collection) => !collection.inspectionComplete);
  if (value.keyReferenceFormat === 'sha1-hex'
    && (value.keyReference === null || !/^[0-9A-F]{40}$/.test(value.keyReference))) return false;
  if (value.structureStatus === 'malformed') return malformed;
  if (malformed) return false;
  return value.structureStatus === (incomplete ? 'indeterminate' : 'well-formed');
}

function isSignatureField(value: unknown): value is PdfSignatureField {
  return isRecord(value)
    && hasExactKeys(value, SIGNATURE_FIELD_KEYS)
    && isString(value.id)
    && isString(value.name)
    && typeof value.signed === 'boolean'
    && isArrayOf(value.widgets, isSignatureWidget);
}

function isSignatureWidget(value: unknown): value is PdfSignatureWidget {
  return isRecord(value)
    && hasExactKeys(value, SIGNATURE_WIDGET_KEYS)
    && Number.isSafeInteger(value.pageIndex)
    && (value.pageIndex as number) >= 0
    && Array.isArray(value.rect)
    && value.rect.length === 4
    && value.rect.every(isFiniteNumber);
}

function isSignatureSummary(value: unknown): value is PdfSignatureSummary {
  return isRecord(value)
    && hasExactKeys(value, SIGNATURE_SUMMARY_KEYS)
    && isString(value.id)
    && isStringArray(value.fieldNames)
    && isOneOf(value.kind, ['approval', 'certification', 'document-timestamp', 'unknown'])
    && isNullableNonnegativeInteger(value.signedRevision)
    && isByteRange(value.byteRange)
    && isArrayOf(value.transforms, isTransform)
    && isArrayOf(value.certificates, isCertificate)
    && isArrayOf(value.timestamps, isTimestamp)
    && isNullableString(value.signerClaim)
    && isNullableString(value.claimedSigningTime)
    && isOneOf(value.integrity, ['intact', 'failed', 'indeterminate', 'unsupported'])
    && isOneOf(value.identityTrust, ['trusted', 'explicitly-trusted', 'untrusted', 'unknown', 'indeterminate'])
    && isOneOf(value.certificateStatus, ['good', 'revoked', 'expired-at-signing', 'expired-now', 'not-yet-valid', 'unknown', 'offline', 'indeterminate'])
    && isOneOf(value.signingTime, ['claimed-only', 'timestamp-verified', 'document-timestamp-verified', 'missing', 'indeterminate'])
    && isOneOf(value.modificationStatus, ['none', 'permitted', 'prohibited', 'indeterminate', 'unable-to-classify'])
    && isOneOf(value.coverage, ['whole-relevant-revision', 'partial-or-ambiguous', 'malformed'])
    && isEvidenceFreshness(value.evidenceFreshness)
    && isQualification(value.qualification)
    && (value.fieldLock === null || isFieldLock(value.fieldLock))
    && isArrayOf(value.issues, isValidationIssue);
}

function isByteRange(value: unknown): value is PdfSignatureByteRange {
  return isRecord(value)
    && hasExactKeys(value, BYTE_RANGE_KEYS)
    && Array.isArray(value.segments)
    && value.segments.every((segment) => Array.isArray(segment)
      && segment.length === 2
      && segment.every((part) => Number.isSafeInteger(part) && part >= 0))
    && isNullableNonnegativeInteger(value.coveredRevisionEnd)
    && typeof value.structurallyValid === 'boolean';
}

function isTransform(value: unknown): value is PdfSignatureTransform {
  return isRecord(value)
    && hasExactKeys(value, TRANSFORM_KEYS)
    && isOneOf(value.method, ['DocMDP', 'FieldMDP', 'UR3', 'unknown'])
    && isCertificationPermission(value.certificationPermission)
    && (value.fieldLock === null || isFieldLock(value.fieldLock))
    && typeof value.parsed === 'boolean';
}

function isCertificate(value: unknown): value is PdfSignatureCertificate {
  return isRecord(value)
    && hasExactKeys(value, CERTIFICATE_KEYS)
    && isString(value.subject)
    && isString(value.issuer)
    && isString(value.serialNumber)
    && isSha256(value.sha256Fingerprint)
    && isString(value.validFrom)
    && isString(value.validTo)
    && isString(value.keyAlgorithm)
    && typeof value.signingCertificate === 'boolean';
}

function isTimestamp(value: unknown): value is PdfSignatureTimestamp {
  return isRecord(value)
    && hasExactKeys(value, TIMESTAMP_KEYS)
    && isOneOf(value.kind, ['signature-timestamp', 'document-timestamp', 'claimed-time', 'unknown'])
    && isNullableString(value.time)
    && typeof value.verified === 'boolean'
    && isNullableString(value.tsaClaim);
}

function isEvidenceFreshness(value: unknown): value is SignatureEvidenceFreshness {
  return isRecord(value)
    && hasExactKeys(value, EVIDENCE_FRESHNESS_KEYS)
    && isOneOf(value.source, ['embedded', 'cached', 'online', 'none', 'indeterminate'])
    && isNullableString(value.producedAt)
    && isNullableString(value.nextUpdateAt);
}

function isQualification(value: unknown): value is SignatureQualification {
  return isRecord(value)
    && hasExactKeys(value, QUALIFICATION_KEYS)
    && isOneOf(value.padesProfile, ['B-B', 'B-T', 'B-LT', 'B-LTA', 'legacy', 'unknown'])
    && typeof value.claimedCompliant === 'boolean'
    && isStringArray(value.limitations);
}

function isFieldLock(value: unknown): value is PdfFieldLock {
  return isRecord(value)
    && hasExactKeys(value, FIELD_LOCK_KEYS)
    && isOneOf(value.action, ['all', 'include', 'exclude', 'unknown'])
    && isStringArray(value.fieldNames);
}

function isTrustContext(value: unknown): value is PdfSignatureTrustContext {
  return isRecord(value)
    && hasExactKeys(value, TRUST_CONTEXT_KEYS)
    && isString(value.policyId)
    && /^[A-Za-z0-9._:-]{1,128}$/.test(value.policyId as string)
    && Number.isSafeInteger(value.policyVersion)
    && (value.policyVersion as number) >= 1
    && isSha256(value.configurationSha256)
    && isString(value.policyName)
    && isUniqueSortedSha256Array(value.configuredExactCertificateFingerprints, 16)
    && (value.onlineSourcesUsed === false || isStringArray(value.onlineSourcesUsed))
    && isStringArray(value.limitations);
}

function isUniqueSortedSha256Array(value: unknown, maxLength: number): value is readonly string[] {
  if (!Array.isArray(value) || value.length > maxLength || !value.every(isSha256)) return false;
  return value.every((entry, index) => index === 0 || value[index - 1] < entry);
}

function isValidationIssue(value: unknown): value is SignatureValidationIssue {
  return isRecord(value)
    && hasExactKeys(value, VALIDATION_ISSUE_KEYS, ['signatureId'])
    && isOneOf(value.code, [
      'MALFORMED_PDF', 'MALFORMED_SIGNATURE', 'INVALID_BYTE_RANGE', 'UNSUPPORTED_SIGNATURE',
      'CRYPTOGRAPHIC_FAILURE', 'TRUST_PATH_UNAVAILABLE', 'REVOCATION_STATUS_UNKNOWN',
      'SIGNING_EKU_UNSUITABLE',
      'TIMESTAMP_INVALID', 'MODIFICATION_PROHIBITED', 'VALIDATION_OFFLINE_INCOMPLETE',
      'ENGINE_UNAVAILABLE', 'ENGINE_PROTOCOL_ERROR', 'RESOURCE_LIMIT', 'CANCELLED', 'TIMEOUT',
      'INTERNAL_ERROR',
    ])
    && isOneOf(value.severity, ['warning', 'error'])
    && isString(value.message)
    && (!Object.prototype.hasOwnProperty.call(value, 'signatureId') || isString(value.signatureId));
}

function isCertificationPermission(value: unknown): value is CertificationPermission {
  return isOneOf(value, [
    'not-certified', 'no-changes', 'form-filling-and-signatures',
    'form-filling-signatures-and-annotations', 'unknown',
  ]);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasExactKeys(
  value: Record<string, unknown>,
  requiredKeys: readonly string[],
  optionalKeys: readonly string[] = [],
): boolean {
  const allowedKeys = new Set([...requiredKeys, ...optionalKeys]);
  const keys = Object.keys(value);
  return requiredKeys.every((key) => Object.prototype.hasOwnProperty.call(value, key))
    && keys.every((key) => allowedKeys.has(key));
}

function isString(value: unknown): value is string {
  return typeof value === 'string';
}

function isNullableString(value: unknown): value is string | null {
  return value === null || isString(value);
}

function isStringArray(value: unknown): value is readonly string[] {
  return Array.isArray(value) && value.every(isString);
}

function isArrayOf<T>(value: unknown, guard: (item: unknown) => item is T): value is readonly T[] {
  return Array.isArray(value) && value.every(guard);
}

function isOneOf<T extends string>(value: unknown, allowed: readonly T[]): value is T {
  return typeof value === 'string' && allowed.includes(value as T);
}

function isNullableNonnegativeInteger(value: unknown): value is number | null {
  return value === null || (Number.isSafeInteger(value) && (value as number) >= 0);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

function isSha256(value: unknown): value is string {
  return typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
}

function isFieldLocked(locks: readonly PdfFieldLock[], fieldName: string): boolean {
  for (const lock of locks) {
    if (lock.action === 'unknown' || lock.action === 'all') return true;
    const listed = lock.fieldNames.includes(fieldName);
    if ((lock.action === 'include' && listed) || (lock.action === 'exclude' && !listed)) return true;
  }
  return false;
}

function decision(
  outcome: SignaturePolicyOutcome,
  reason: SignaturePolicyReason,
  sourceReadOnly: boolean,
): SignaturePolicyDecision {
  return { outcome, reason, sourceReadOnly };
}
