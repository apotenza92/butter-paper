import { ChevronDown } from 'lucide-react';
import { useEffect, useId, useState } from 'react';
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select';
import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from '@/components/ui/popover';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import {
  BLANK_PDF_PAPER_PRESETS,
  formatBlankPdfPaperPresetOption,
  resolveBlankPdfDimensions,
  type BlankPdfOrientation,
  type BlankPdfPaperPreset,
  type BlankPdfSettings,
} from './blankPdfSettings';
import { SplitButtonSegment } from './domain-ui/SplitButtonSegment';

interface BlankPdfSettingsPopoverProps {
  settings: BlankPdfSettings;
  onSettingsChange: (settings: BlankPdfSettings) => void;
}

export function BlankPdfSettingsPopover({ settings, onSettingsChange }: BlankPdfSettingsPopoverProps) {
  const paperSizeId = useId();
  const widthId = useId();
  const heightId = useId();
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState(settings);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) setDraft(settings);
  }, [open, settings]);

  function handleOpenChange(nextOpen: boolean): void {
    if (nextOpen) {
      setDraft(settings);
      setError(null);
    }
    setOpen(nextOpen);
  }

  function updateSettings(nextSettings: BlankPdfSettings): void {
    setDraft(nextSettings);
    try {
      resolveBlankPdfDimensions(nextSettings);
      onSettingsChange(nextSettings);
      setError(null);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Unable to save the blank PDF default.');
    }
  }

  return (
    <Popover open={open} onOpenChange={handleOpenChange}>
      <Tooltip disabled={open}>
        <PopoverTrigger
          render={(
            <TooltipTrigger
              render={(
                <SplitButtonSegment
                  type="button"
                  size="icon"
                  aria-label="Blank PDF settings"
                  data-testid="document-tab-new-pdf-settings"
                >
                  <ChevronDown data-icon="inline-start" aria-hidden="true" />
                </SplitButtonSegment>
              )}
            />
          )}
        />
        <TooltipContent>Blank PDF settings</TooltipContent>
      </Tooltip>
      <PopoverContent
        align="start"
        className="w-[320px]"
        data-testid="new-blank-pdf-settings"
        finalFocus={() => document.querySelector<HTMLElement>('[data-testid="document-tab-new-pdf-settings"]')}
      >
        <PopoverHeader>
          <PopoverTitle>Blank PDF default</PopoverTitle>
          <PopoverDescription>Choose the settings used for new PDFs. Changes are saved automatically.</PopoverDescription>
        </PopoverHeader>
        <FieldGroup className="gap-4">
          <Field>
            <FieldLabel htmlFor={paperSizeId}>Paper size</FieldLabel>
            <NativeSelect
              id={paperSizeId}
              className="w-full"
              value={draft.preset}
              data-testid="new-blank-pdf-paper-size"
              onChange={(event) => updateSettings({
                ...draft,
                preset: event.currentTarget.value as BlankPdfPaperPreset,
              })}
            >
              {(Object.keys(BLANK_PDF_PAPER_PRESETS) as Array<Exclude<BlankPdfPaperPreset, 'custom'>>).map((value) => (
                <NativeSelectOption key={value} value={value}>
                  {formatBlankPdfPaperPresetOption(value, draft.orientation)}
                </NativeSelectOption>
              ))}
              <NativeSelectOption value="custom">Custom</NativeSelectOption>
            </NativeSelect>
          </Field>

          {draft.preset === 'custom' ? (
            <FieldGroup className="grid grid-cols-2 gap-3">
              <DimensionField
                id={widthId}
                label="Width (mm)"
                value={draft.customWidth}
                invalid={error !== null}
                testId="new-blank-pdf-width"
                onChange={(customWidth) => updateSettings({ ...draft, customWidth })}
              />
              <DimensionField
                id={heightId}
                label="Height (mm)"
                value={draft.customHeight}
                invalid={error !== null}
                testId="new-blank-pdf-height"
                onChange={(customHeight) => updateSettings({ ...draft, customHeight })}
              />
            </FieldGroup>
          ) : (
            <Field>
              <FieldLabel>Orientation</FieldLabel>
              <ToggleGroup
                aria-label="Orientation"
                className="grid w-full grid-cols-2"
                spacing={0}
                variant="outline"
                value={[draft.orientation]}
                onValueChange={(values) => {
                  const orientation = values[0] as BlankPdfOrientation | undefined;
                  if (orientation) updateSettings({ ...draft, orientation });
                }}
              >
                <ToggleGroupItem className="w-full" value="portrait" data-testid="new-blank-pdf-portrait">Portrait</ToggleGroupItem>
                <ToggleGroupItem className="w-full" value="landscape" data-testid="new-blank-pdf-landscape">Landscape</ToggleGroupItem>
              </ToggleGroup>
            </Field>
          )}
        </FieldGroup>
        {error ? <FieldError data-testid="new-blank-pdf-error">{error}</FieldError> : null}
      </PopoverContent>
    </Popover>
  );
}

function DimensionField({
  id,
  label,
  value,
  invalid,
  testId,
  onChange,
}: {
  id: string;
  label: string;
  value: string;
  invalid: boolean;
  testId: string;
  onChange: (value: string) => void;
}) {
  return (
    <Field data-invalid={invalid || undefined}>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Input
        id={id}
        type="number"
        min="10"
        max="5000"
        step="0.1"
        inputMode="decimal"
        value={value}
        aria-invalid={invalid || undefined}
        data-testid={testId}
        onChange={(event) => onChange(event.currentTarget.value)}
      />
    </Field>
  );
}
