import { LockKeyhole, ShieldAlert, ShieldCheck } from 'lucide-react';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import type {
  LoadedDocumentSignatureProtection,
  LoadedDocumentSignatureValidation,
} from '../../../shared/protocol';

interface SignatureStatusBannerProps {
  protection?: LoadedDocumentSignatureProtection;
  validation?: LoadedDocumentSignatureValidation;
}

export function SignatureStatusBanner({ protection, validation }: SignatureStatusBannerProps) {
  if (!protection || (protection.status === 'unsigned' && validation?.status !== 'unavailable')) {
    return null;
  }

  const unavailable = validation?.status !== 'complete';
  const title = unavailable
    ? 'Signature status unavailable — document is read-only'
    : protection.status === 'certified'
      ? 'Certified PDF — document is read-only'
      : protection.status === 'potentially-signed'
        ? 'Potential signature detected — document is read-only'
        : protection.status === 'indeterminate'
          ? 'Signature status indeterminate — document is read-only'
          : 'Signed PDF — document is read-only';
  const Icon = unavailable ? ShieldAlert : protection.status === 'certified' ? ShieldCheck : LockKeyhole;
  const description = unavailable
    ? `${validation?.message ?? 'Offline PDF signature validation is unavailable.'} No trust or integrity conclusion is being reported.`
    : `Offline validation complete. ${validation.signatureCount} signature${validation.signatureCount === 1 ? '' : 's'} found; ${validation.issueCount} reported validation issue${validation.issueCount === 1 ? '' : 's'}. No trust or integrity conclusion is implied by this summary.`;

  return (
    <Alert
      className="rounded-none border-x-0 border-t-0 px-4 py-2"
      data-testid="signature-status-banner"
    >
      <Icon aria-hidden="true" />
      <AlertTitle className="flex items-center gap-2">
        {title}
        <Badge variant={unavailable ? 'destructive' : 'outline'}>
          {unavailable ? 'Unavailable' : 'Offline'}
        </Badge>
      </AlertTitle>
      <AlertDescription>{description}</AlertDescription>
    </Alert>
  );
}
