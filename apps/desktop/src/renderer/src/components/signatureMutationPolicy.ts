import type { LoadedDocumentSignatureProtection } from '../../../shared/protocol';

/**
 * Signed, uncertain, or unavailable-validation documents must not reach any
 * renderer mutation route. Main IPC remains the final authority, but keeping
 * the renderer in the same fail-closed state prevents misleading dirty UI and
 * accidental annotation drafts.
 */
export function isDocumentMutationDisabled(
  protection: LoadedDocumentSignatureProtection | undefined,
): boolean {
  return protection?.sourceReadOnly !== false || protection.status !== 'unsigned';
}
