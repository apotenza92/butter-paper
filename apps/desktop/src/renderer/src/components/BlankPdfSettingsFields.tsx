import { useId } from 'react';
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import {
  BLANK_PDF_PAPER_PRESETS,
  formatBlankPdfPaperPresetOption,
  type BlankPdfOrientation,
  type BlankPdfPaperPreset,
  type BlankPdfSettings,
} from './blankPdfSettings';

interface BlankPdfSettingsFieldsProps {
  settings: BlankPdfSettings;
  error: string | null;
  testIdPrefix: string;
  onSettingsChange: (settings: BlankPdfSettings) => void;
}

export function BlankPdfSettingsFields({
  settings,
  error,
  testIdPrefix,
  onSettingsChange,
}: BlankPdfSettingsFieldsProps) {
  const paperSizeId = useId();
  const widthId = useId();
  const heightId = useId();

  return (
    <>
      <FieldGroup className="gap-4">
        <Field>
          <FieldLabel htmlFor={paperSizeId}>Paper size</FieldLabel>
          <NativeSelect
            id={paperSizeId}
            className="w-full"
            value={settings.preset}
            data-testid={`${testIdPrefix}-paper-size`}
            onChange={(event) => onSettingsChange({
              ...settings,
              preset: event.currentTarget.value as BlankPdfPaperPreset,
            })}
          >
            {(Object.keys(BLANK_PDF_PAPER_PRESETS) as Array<Exclude<BlankPdfPaperPreset, 'custom'>>).map((value) => (
              <NativeSelectOption key={value} value={value}>
                {formatBlankPdfPaperPresetOption(value, settings.orientation)}
              </NativeSelectOption>
            ))}
            <NativeSelectOption value="custom">Custom</NativeSelectOption>
          </NativeSelect>
        </Field>

        {settings.preset === 'custom' ? (
          <FieldGroup className="grid grid-cols-2 gap-3">
            <DimensionField
              id={widthId}
              label="Width (mm)"
              value={settings.customWidth}
              invalid={error !== null}
              testId={`${testIdPrefix}-width`}
              onChange={(customWidth) => onSettingsChange({ ...settings, customWidth })}
            />
            <DimensionField
              id={heightId}
              label="Height (mm)"
              value={settings.customHeight}
              invalid={error !== null}
              testId={`${testIdPrefix}-height`}
              onChange={(customHeight) => onSettingsChange({ ...settings, customHeight })}
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
              value={[settings.orientation]}
              onValueChange={(values) => {
                const orientation = values[0] as BlankPdfOrientation | undefined;
                if (orientation) onSettingsChange({ ...settings, orientation });
              }}
            >
              <ToggleGroupItem className="w-full" value="portrait" data-testid={`${testIdPrefix}-portrait`}>Portrait</ToggleGroupItem>
              <ToggleGroupItem className="w-full" value="landscape" data-testid={`${testIdPrefix}-landscape`}>Landscape</ToggleGroupItem>
            </ToggleGroup>
          </Field>
        )}
      </FieldGroup>
      {error ? <FieldError data-testid={`${testIdPrefix}-error`}>{error}</FieldError> : null}
    </>
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
