import type {
  CertificationPermission,
  SigningAppearanceMode,
  SigningApprovalRequest,
  SigningCapabilitySnapshot,
  SigningDigestAlgorithm,
  SigningFieldRequest,
  SigningIdentitySummary,
  SigningOperation,
  SigningRectangle,
} from '../../../shared/signingProtocol';

export interface SigningNewFieldDefaults {
  readonly name: string;
  readonly pageIndex: number;
  readonly pageRotation: 0 | 90 | 180 | 270;
  readonly rect: SigningRectangle;
  readonly lock: Extract<SigningFieldRequest, { mode: 'new' }>['lock'];
}

export interface SigningAppearanceContext {
  readonly defaultMode: SigningAppearanceMode;
  /** Opaque main-owned handle only. This is never a path or raw asset data. */
  readonly visibleAssetHandle?: string;
  readonly visibleAssetLabel?: string;
}

export interface SigningApprovalContext {
  readonly documentHandle: string;
  readonly targetHandle: string;
  readonly identity: SigningIdentitySummary;
  readonly capabilities: SigningCapabilitySnapshot;
  readonly sourceIsUnsigned: boolean;
  readonly existingFieldNames: readonly string[];
  readonly newFieldDefaults: SigningNewFieldDefaults;
  readonly appearance: SigningAppearanceContext;
}

export interface SigningApprovalDraft {
  readonly operation: SigningOperation;
  readonly digestAlgorithm: SigningDigestAlgorithm;
  readonly fieldMode: SigningFieldRequest['mode'];
  readonly existingFieldName: string;
  readonly newFieldName: string;
  readonly appearanceMode: SigningAppearanceMode;
  readonly certificationPermission: CertificationPermission;
  readonly reason: string;
  readonly location: string;
  readonly contact: string;
}

export function createInitialSigningApprovalDraft(context: SigningApprovalContext): SigningApprovalDraft {
  const operation = resolveInitialOperation(context.capabilities, context.sourceIsUnsigned);
  const firstExistingField = context.existingFieldNames[0] ?? '';

  return {
    operation,
    digestAlgorithm: 'SHA-256',
    fieldMode: firstExistingField ? 'existing' : 'new',
    existingFieldName: firstExistingField,
    newFieldName: context.newFieldDefaults.name,
    appearanceMode: context.appearance.defaultMode,
    certificationPermission: 'no-changes',
    reason: '',
    location: '',
    contact: '',
  };
}

export function resolveInitialOperation(
  capabilities: SigningCapabilitySnapshot,
  sourceIsUnsigned: boolean,
): SigningOperation {
  if (capabilities.certificateSign) {
    return 'sign';
  }
  if (capabilities.certify && sourceIsUnsigned) {
    return 'certify';
  }
  return 'sign';
}

export function validateSigningApprovalDraft(
  draft: SigningApprovalDraft,
  context: SigningApprovalContext,
): readonly string[] {
  const errors: string[] = [];
  const canSign = context.capabilities.certificateSign;
  const canCertify = context.capabilities.certify && context.sourceIsUnsigned;

  if (draft.operation === 'sign' && !canSign) {
    errors.push('Signing is unavailable for this document or identity.');
  }

  if (draft.operation === 'certify' && !canCertify) {
    errors.push(
      context.capabilities.certify
        ? 'Certification requires an unsigned source document.'
        : 'Certification is unavailable for this document or identity.',
    );
  }

  if (draft.fieldMode === 'existing') {
    if (!context.existingFieldNames.includes(draft.existingFieldName)) {
      errors.push('Select an available signature field.');
    }
  } else if (!draft.newFieldName.trim()) {
    errors.push('Enter a name for the new signature field.');
  }

  if (draft.appearanceMode === 'visible' && !context.appearance.visibleAssetHandle) {
    errors.push('A visible appearance is unavailable until a protected appearance asset is provided.');
  }

  if (!context.documentHandle || !context.targetHandle) {
    errors.push('The document approval handles are unavailable.');
  }

  if (!context.identity.identityHandle || !context.identity.certificateSha256) {
    errors.push('The selected identity certificate is unavailable.');
  }

  return errors;
}

export function buildSigningApprovalRequest(
  draft: SigningApprovalDraft,
  context: SigningApprovalContext,
): SigningApprovalRequest | null {
  if (validateSigningApprovalDraft(draft, context).length > 0) {
    return null;
  }

  const field: SigningFieldRequest = draft.fieldMode === 'existing'
    ? { mode: 'existing', name: draft.existingFieldName }
    : {
        mode: 'new',
        name: draft.newFieldName.trim(),
        pageIndex: context.newFieldDefaults.pageIndex,
        pageRotation: context.newFieldDefaults.pageRotation,
        rect: context.newFieldDefaults.rect,
        lock: context.newFieldDefaults.lock,
      };
  const appearance = draft.appearanceMode === 'invisible'
    ? { mode: 'invisible' as const }
    : { mode: 'visible' as const, assetHandle: context.appearance.visibleAssetHandle as string };
  const optionalMetadata = {
    ...(draft.reason.trim() ? { reason: draft.reason.trim() } : {}),
    ...(draft.location.trim() ? { location: draft.location.trim() } : {}),
    ...(draft.contact.trim() ? { contact: draft.contact.trim() } : {}),
  };
  const base = {
    protocolVersion: 1 as const,
    profile: 'PAdES-B-B' as const,
    documentHandle: context.documentHandle,
    targetHandle: context.targetHandle,
    identityHandle: context.identity.identityHandle,
    certificateSha256: context.identity.certificateSha256,
    digestAlgorithm: draft.digestAlgorithm,
    field,
    appearance,
    ...optionalMetadata,
  };

  return draft.operation === 'certify'
    ? { ...base, operation: 'certify', certificationPermission: draft.certificationPermission }
    : { ...base, operation: 'sign' };
}
