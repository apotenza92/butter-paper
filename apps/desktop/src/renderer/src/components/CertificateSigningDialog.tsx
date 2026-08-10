import { useEffect, useId, useState } from 'react';
import type { FormEvent } from 'react';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Spinner } from '@/components/ui/spinner';
import { Checkbox } from '@/components/ui/checkbox';
import type {
  CertificationPermission,
  SigningApprovalRequest,
  SigningCapabilitySnapshot,
  SigningDigestAlgorithm,
  SigningIdentitySummary,
  SigningOperation,
} from '../../../shared/signingProtocol';
import {
  buildSigningApprovalRequest,
  createInitialSigningApprovalDraft,
  type SigningApprovalContext,
  type SigningApprovalDraft,
  validateSigningApprovalDraft,
} from './signingApprovalState';

export interface CertificateSigningDialogProps extends SigningApprovalContext {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
  readonly onApprove: (request: SigningApprovalRequest) => void;
  readonly onCancel?: () => void;
  /** A main-owned failure rendered after the approval request is rejected. */
  readonly failureMessage?: string | null;
}

const DIGEST_OPTIONS: readonly { value: SigningDigestAlgorithm; label: string }[] = [
  { value: 'SHA-256', label: 'SHA-256' },
  { value: 'SHA-384', label: 'SHA-384' },
  { value: 'SHA-512', label: 'SHA-512' },
];

const CERTIFICATION_PERMISSION_OPTIONS: readonly { value: CertificationPermission; label: string }[] = [
  { value: 'no-changes', label: 'No changes' },
  { value: 'form-filling-and-signatures', label: 'Form filling and signatures' },
  { value: 'form-filling-signatures-and-annotations', label: 'Form filling, signatures, and annotations' },
];

export function CertificateSigningDialog({
  open,
  onOpenChange,
  onApprove,
  onCancel,
  failureMessage = null,
  ...context
}: CertificateSigningDialogProps) {
  const [draft, setDraft] = useState<SigningApprovalDraft>(() => createInitialSigningApprovalDraft(context));
  const [humanApproval, setHumanApproval] = useState(false);
  const [validationErrors, setValidationErrors] = useState<readonly string[]>([]);
  const [requestSent, setRequestSent] = useState(false);
  const [localFailure, setLocalFailure] = useState<string | null>(null);
  const operationId = useId();
  const digestId = useId();
  const fieldModeId = useId();
  const existingFieldId = useId();
  const newFieldNameId = useId();
  const appearanceId = useId();
  const certificationPermissionId = useId();
  const reasonId = useId();
  const locationId = useId();
  const contactId = useId();

  const canSign = context.capabilities.certificateSign;
  const canCertify = context.capabilities.certify && context.sourceIsUnsigned;
  const effectiveFailure = failureMessage ?? localFailure;
  const approvalWaiting = requestSent && !effectiveFailure;
  const errors = validationErrors;

  useEffect(() => {
    if (open) {
      setDraft(createInitialSigningApprovalDraft(context));
      setHumanApproval(false);
      setValidationErrors([]);
      setRequestSent(false);
      setLocalFailure(null);
    }
  }, [
    open,
    context.appearance.defaultMode,
    context.appearance.visibleAssetHandle,
    context.capabilities.certificateSign,
    context.capabilities.certify,
    context.documentHandle,
    context.existingFieldNames,
    context.identity,
    context.newFieldDefaults,
    context.sourceIsUnsigned,
    context.targetHandle,
  ]);

  function updateDraft<K extends keyof SigningApprovalDraft>(key: K, value: SigningApprovalDraft[K]): void {
    setDraft((current) => ({ ...current, [key]: value }));
    setValidationErrors([]);
    setLocalFailure(null);
    setRequestSent(false);
  }

  function handleCancel(): void {
    onCancel?.();
    onOpenChange(false);
  }

  function handleApprove(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    const nextErrors = [...validateSigningApprovalDraft(draft, context)];
    if (!humanApproval) {
      nextErrors.push('Confirm that you have reviewed the complete approval details.');
    }
    setValidationErrors(nextErrors);
    if (nextErrors.length > 0) {
      return;
    }

    const request = buildSigningApprovalRequest(draft, context);
    if (!request) {
      setValidationErrors(['The approval details changed and must be reviewed again.']);
      return;
    }

    try {
      onApprove(request);
      setRequestSent(true);
      setLocalFailure(null);
    } catch (caught) {
      setRequestSent(false);
      setLocalFailure(caught instanceof Error ? caught.message : 'The approval request could not be sent.');
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          handleCancel();
        } else {
          onOpenChange(nextOpen);
        }
      }}
    >
      <DialogContent
        data-testid="certificate-signing-dialog"
        className="max-h-[calc(100vh-2rem)] w-[680px] max-w-[calc(100%-2rem)] overflow-hidden sm:max-w-[680px]"
        showCloseButton={false}
      >
        <DialogHeader>
          <DialogTitle>Review certificate approval</DialogTitle>
          <DialogDescription>
            Confirm the semantic operation and protected identity before a signing request is emitted.
          </DialogDescription>
        </DialogHeader>

        <form
          className="flex min-h-0 flex-col gap-4"
          aria-busy={approvalWaiting || undefined}
          onSubmit={handleApprove}
        >
          <div className="min-h-0 overflow-y-auto pr-1">
            <FieldGroup className="gap-4">
              <Card size="sm">
                <CardHeader>
                  <CardTitle>Operation</CardTitle>
                  <CardDescription>Signing and certification are distinct approval semantics.</CardDescription>
                </CardHeader>
                <CardContent>
                  <FieldGroup className="gap-4">
                    <Field data-invalid={errors.some((error) => error.includes('Signing') || error.includes('Certification')) || undefined}>
                      <FieldLabel htmlFor={operationId}>Operation</FieldLabel>
                      <Select
                        value={draft.operation}
                        onValueChange={(value) => {
                          if (value) updateDraft('operation', value as SigningOperation);
                        }}
                      >
                        <SelectTrigger id={operationId} className="w-full" aria-invalid={errors.some((error) => error.includes('Signing') || error.includes('Certification')) || undefined}>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent align="start">
                          <SelectGroup>
                            <SelectItem value="sign" disabled={!canSign}>Sign — add an approval signature</SelectItem>
                            <SelectItem value="certify" disabled={!canCertify}>Certify — establish document permissions</SelectItem>
                          </SelectGroup>
                        </SelectContent>
                      </Select>
                      <FieldDescription>
                        Profile: <strong>PAdES-B-B</strong>. {canCertify
                          ? 'Certification is available only for an unsigned source and may restrict later changes.'
                          : 'Certification is unavailable for this document or identity.'}
                      </FieldDescription>
                    </Field>
                    <Field>
                      <FieldLabel htmlFor={digestId}>Digest algorithm</FieldLabel>
                      <Select
                        value={draft.digestAlgorithm}
                        onValueChange={(value) => {
                          if (value) updateDraft('digestAlgorithm', value as SigningDigestAlgorithm);
                        }}
                      >
                        <SelectTrigger id={digestId} className="w-full">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent align="start">
                          <SelectGroup>
                            {DIGEST_OPTIONS.map((option) => <SelectItem key={option.value} value={option.value}>{option.label}</SelectItem>)}
                          </SelectGroup>
                        </SelectContent>
                      </Select>
                    </Field>
                  </FieldGroup>
                </CardContent>
              </Card>

              <Card size="sm">
                <CardHeader>
                  <CardTitle>Identity certificate</CardTitle>
                  <CardDescription>Public certificate metadata only. Private-key material and passwords never enter renderer state.</CardDescription>
                </CardHeader>
                <CardContent>
                  <dl className="grid gap-3 sm:grid-cols-2">
                    <SummaryItem label="Subject" value={context.identity.subject} />
                    <SummaryItem label="Issuer" value={context.identity.issuer} />
                    <SummaryItem label="Serial number" value={context.identity.serialNumber} />
                    <SummaryItem label="Key algorithm" value={context.identity.keyAlgorithm} />
                    <SummaryItem label="Valid from" value={context.identity.validFrom} />
                    <SummaryItem label="Valid to" value={context.identity.validTo} />
                    <SummaryItem label="Certificate SHA-256" value={context.identity.certificateSha256} wide />
                    <SummaryItem label="Protection" value="Private key not exported · password not remembered" wide />
                  </dl>
                </CardContent>
              </Card>

              <Card size="sm">
                <CardHeader>
                  <CardTitle>Signature field and appearance</CardTitle>
                  <CardDescription>Choose where the signature is applied and whether its protected appearance is visible.</CardDescription>
                </CardHeader>
                <CardContent>
                  <FieldSet>
                    <FieldLegend>Field</FieldLegend>
                    <FieldGroup className="gap-4">
                      <Field data-invalid={errors.some((error) => error.includes('signature field')) || undefined}>
                        <FieldLabel htmlFor={fieldModeId}>Field mode</FieldLabel>
                        <Select
                          value={draft.fieldMode}
                          onValueChange={(value) => {
                            if (value) updateDraft('fieldMode', value as SigningApprovalDraft['fieldMode']);
                          }}
                        >
                          <SelectTrigger id={fieldModeId} className="w-full" aria-invalid={errors.some((error) => error.includes('signature field')) || undefined}>
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent align="start">
                            <SelectGroup>
                              <SelectItem value="existing" disabled={context.existingFieldNames.length === 0}>Use an existing field</SelectItem>
                              <SelectItem value="new">Create a new field</SelectItem>
                            </SelectGroup>
                          </SelectContent>
                        </Select>
                      </Field>
                      {draft.fieldMode === 'existing' ? (
                        <Field data-invalid={errors.some((error) => error.includes('signature field')) || undefined}>
                          <FieldLabel htmlFor={existingFieldId}>Existing signature field</FieldLabel>
                          <Select
                            value={draft.existingFieldName}
                            onValueChange={(value) => {
                              if (value) updateDraft('existingFieldName', value);
                            }}
                          >
                            <SelectTrigger id={existingFieldId} className="w-full" aria-invalid={errors.some((error) => error.includes('signature field')) || undefined}>
                              <SelectValue />
                            </SelectTrigger>
                            <SelectContent align="start">
                              <SelectGroup>
                                {context.existingFieldNames.map((name) => <SelectItem key={name} value={name}>{name}</SelectItem>)}
                              </SelectGroup>
                            </SelectContent>
                          </Select>
                        </Field>
                      ) : (
                        <Field data-invalid={errors.some((error) => error.includes('new signature field')) || undefined}>
                          <FieldLabel htmlFor={newFieldNameId}>New field name</FieldLabel>
                          <Input
                            id={newFieldNameId}
                            value={draft.newFieldName}
                            aria-invalid={errors.some((error) => error.includes('new signature field')) || undefined}
                            onChange={(event) => updateDraft('newFieldName', event.target.value)}
                          />
                          <FieldDescription>
                            Page {context.newFieldDefaults.pageIndex + 1}, rotation {context.newFieldDefaults.pageRotation}°, rectangle {formatRectangle(context.newFieldDefaults.rect)}. {formatLock(context.newFieldDefaults.lock)}
                          </FieldDescription>
                        </Field>
                      )}
                    </FieldGroup>
                  </FieldSet>

                  <FieldSet className="mt-5">
                    <FieldLegend>Appearance</FieldLegend>
                    <FieldGroup className="gap-4">
                      <Field data-invalid={errors.some((error) => error.includes('visible appearance')) || undefined}>
                        <FieldLabel htmlFor={appearanceId}>Appearance</FieldLabel>
                        <Select
                          value={draft.appearanceMode}
                          onValueChange={(value) => {
                            if (value) updateDraft('appearanceMode', value as SigningApprovalDraft['appearanceMode']);
                          }}
                        >
                          <SelectTrigger id={appearanceId} className="w-full" aria-invalid={errors.some((error) => error.includes('visible appearance')) || undefined}>
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent align="start">
                            <SelectGroup>
                              <SelectItem value="invisible">Invisible signature</SelectItem>
                              <SelectItem value="visible" disabled={!context.appearance.visibleAssetHandle}>Visible protected appearance</SelectItem>
                            </SelectGroup>
                          </SelectContent>
                        </Select>
                        <FieldDescription>
                          {draft.appearanceMode === 'visible'
                            ? context.appearance.visibleAssetLabel ?? 'A protected appearance asset will be referenced by opaque handle.'
                            : 'No visual mark is embedded in the signature field.'}
                        </FieldDescription>
                      </Field>
                    </FieldGroup>
                  </FieldSet>
                </CardContent>
              </Card>

              {draft.operation === 'certify' ? (
                <Card size="sm">
                  <CardHeader>
                    <CardTitle>Certification permission</CardTitle>
                    <CardDescription>Set the DocMDP permission that will be requested for the certified output.</CardDescription>
                  </CardHeader>
                  <CardContent>
                    <Field>
                      <FieldLabel htmlFor={certificationPermissionId}>Allowed changes after certification</FieldLabel>
                      <Select
                        value={draft.certificationPermission}
                        onValueChange={(value) => {
                          if (value) updateDraft('certificationPermission', value as CertificationPermission);
                        }}
                      >
                        <SelectTrigger id={certificationPermissionId} className="w-full">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent align="start">
                          <SelectGroup>
                            {CERTIFICATION_PERMISSION_OPTIONS.map((option) => <SelectItem key={option.value} value={option.value}>{option.label}</SelectItem>)}
                          </SelectGroup>
                        </SelectContent>
                      </Select>
                    </Field>
                  </CardContent>
                </Card>
              ) : null}

              <Card size="sm">
                <CardHeader>
                  <CardTitle>Signature metadata</CardTitle>
                  <CardDescription>Optional text included in the semantic approval request.</CardDescription>
                </CardHeader>
                <CardContent>
                  <FieldGroup className="gap-4 sm:grid sm:grid-cols-2">
                    <Field>
                      <FieldLabel htmlFor={reasonId}>Reason</FieldLabel>
                      <Input id={reasonId} value={draft.reason} onChange={(event) => updateDraft('reason', event.target.value)} />
                    </Field>
                    <Field>
                      <FieldLabel htmlFor={locationId}>Location</FieldLabel>
                      <Input id={locationId} value={draft.location} onChange={(event) => updateDraft('location', event.target.value)} />
                    </Field>
                    <Field className="sm:col-span-2">
                      <FieldLabel htmlFor={contactId}>Contact</FieldLabel>
                      <Input id={contactId} value={draft.contact} onChange={(event) => updateDraft('contact', event.target.value)} />
                    </Field>
                  </FieldGroup>
                </CardContent>
              </Card>

              <Alert>
                <AlertTitle>Offline-only signing capability</AlertTitle>
                <AlertDescription>
                  This flow is limited to local PAdES-B-B approval. Online validation, timestamps, long-term validation, PKCS#11, and batch signing are unavailable; no stronger assurance is implied.
                </AlertDescription>
              </Alert>

              <Card size="sm">
                <CardHeader>
                  <CardTitle>Source and output</CardTitle>
                  <CardDescription>Approval is directed to a new output target and does not replace the source document.</CardDescription>
                </CardHeader>
                <CardContent className="flex flex-wrap items-center gap-2">
                  <Badge variant="secondary">Source preserved</Badge>
                  <span className="text-muted-foreground">A separate output will be created after independent validation.</span>
                </CardContent>
                <CardFooter className="text-muted-foreground">
                  No output path, password, PFX data, private-key handle, or file content is collected here.
                </CardFooter>
              </Card>

              {effectiveFailure ? (
                <Alert variant="destructive" data-testid="certificate-signing-failure">
                  <AlertTitle>Approval request failed</AlertTitle>
                  <AlertDescription>{effectiveFailure} No signed output is being reported.</AlertDescription>
                </Alert>
              ) : null}

              {approvalWaiting ? (
                <Alert data-testid="certificate-signing-pending">
                  <AlertTitle>Approval request emitted</AlertTitle>
                  <AlertDescription>The signing service has not confirmed a signed output. Keep this review open until the result is independently reported.</AlertDescription>
                </Alert>
              ) : null}

              {errors.length > 0 ? (
                <FieldError errors={errors.map((message) => ({ message }))} />
              ) : null}

              <Field orientation="horizontal" data-disabled={approvalWaiting || undefined}>
                <Checkbox
                  id="certificate-signing-human-approval"
                  checked={humanApproval}
                  disabled={approvalWaiting}
                  onCheckedChange={(checked) => {
                    setHumanApproval(checked === true);
                    setValidationErrors([]);
                  }}
                />
                <FieldLabel htmlFor="certificate-signing-human-approval">
                  I have reviewed the operation, certificate, field, appearance, permissions, and source-preservation details.
                </FieldLabel>
              </Field>
            </FieldGroup>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" disabled={approvalWaiting} onClick={handleCancel}>Cancel</Button>
            <Button
              type="submit"
              data-testid="certificate-signing-approve"
              disabled={approvalWaiting || !canSign && !canCertify}
            >
              {approvalWaiting ? <Spinner data-icon="inline-start" aria-hidden="true" /> : null}
              {approvalWaiting ? 'Waiting for signing result…' : effectiveFailure ? 'Retry approval request' : 'Approve and send request'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function SummaryItem({ label, value, wide = false }: { label: string; value: string; wide?: boolean }) {
  return (
    <div className={wide ? 'sm:col-span-2' : undefined}>
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="break-words font-medium">{value}</dd>
    </div>
  );
}

function formatRectangle(rect: SigningApprovalContext['newFieldDefaults']['rect']): string {
  return `${rect.width} × ${rect.height} at (${rect.x}, ${rect.y})`;
}

function formatLock(lock: SigningApprovalContext['newFieldDefaults']['lock']): string {
  if (!lock) {
    return 'No field lock.';
  }
  if (lock.action === 'all') {
    return 'All other fields are locked.';
  }
  return `${lock.action === 'include' ? 'Only' : 'All except'} ${lock.fieldNames.join(', ')} are locked.`;
}
