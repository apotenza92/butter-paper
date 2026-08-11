import { useId } from 'react';
import { CheckIcon } from 'lucide-react';
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { NativeSelect, NativeSelectOption } from '@/components/ui/native-select';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import {
  BLANK_PDF_PAPER_PRESETS,
  formatBlankPdfPaperPresetOption,
  type BlankPdfOrientation,
  type BlankPdfPaperPreset,
  type BlankPdfPatternColorPreset,
  type BlankPdfPatternSpacingPreset,
  type BlankPdfPatternType,
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
  const patternSpacingId = useId();
  const customPatternSpacingId = useId();
  const patternColorPresetId = useId();
  const customPatternColorId = useId();

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
              <ToggleGroupItem className="w-full" value="portrait" data-testid={`${testIdPrefix}-portrait`}>
                Portrait
                <CheckIcon
                  data-icon="inline-end"
                  visibility={settings.orientation === 'portrait' ? 'visible' : 'hidden'}
                  aria-hidden="true"
                />
              </ToggleGroupItem>
              <ToggleGroupItem className="w-full" value="landscape" data-testid={`${testIdPrefix}-landscape`}>
                Landscape
                <CheckIcon
                  data-icon="inline-end"
                  visibility={settings.orientation === 'landscape' ? 'visible' : 'hidden'}
                  aria-hidden="true"
                />
              </ToggleGroupItem>
            </ToggleGroup>
          </Field>
        )}

        <Field>
          <FieldLabel>Paper background</FieldLabel>
          <ToggleGroup
            aria-label="Paper background"
            className="grid w-full grid-cols-3"
            spacing={2}
            variant="outline"
            value={[settings.patternType]}
            onValueChange={(values) => {
              const patternType = values[0] as BlankPdfPatternType | undefined;
              if (patternType) onSettingsChange({ ...settings, patternType });
            }}
          >
            <ToggleGroupItem className="w-full" value="blank" data-testid={`${testIdPrefix}-background-blank`}>Blank</ToggleGroupItem>
            <ToggleGroupItem className="w-full" value="dots" data-testid={`${testIdPrefix}-background-dots`}>Dots</ToggleGroupItem>
            <ToggleGroupItem className="w-full" value="grid" data-testid={`${testIdPrefix}-background-grid`}>Grid</ToggleGroupItem>
            <ToggleGroupItem className="w-full" value="lined" data-testid={`${testIdPrefix}-background-lined`}>Lined</ToggleGroupItem>
            <ToggleGroupItem className="w-full" value="isometric" data-testid={`${testIdPrefix}-background-isometric`}>Isometric</ToggleGroupItem>
            <ToggleGroupItem className="w-full" value="triangle" data-testid={`${testIdPrefix}-background-triangle`}>Triangle</ToggleGroupItem>
          </ToggleGroup>
        </Field>

        {settings.patternType !== 'blank' ? (
          <>
            <FieldGroup className="grid grid-cols-2 gap-3">
              <Field>
                <FieldLabel htmlFor={patternSpacingId}>Spacing</FieldLabel>
                <NativeSelect
                  id={patternSpacingId}
                  className="w-full"
                  value={settings.patternSpacingPreset}
                  data-testid={`${testIdPrefix}-pattern-spacing`}
                  onChange={(event) => onSettingsChange({
                    ...settings,
                    patternSpacingPreset: event.currentTarget.value as BlankPdfPatternSpacingPreset,
                  })}
                >
                  <NativeSelectOption value="5">5 mm</NativeSelectOption>
                  <NativeSelectOption value="10">10 mm</NativeSelectOption>
                  <NativeSelectOption value="25">25 mm</NativeSelectOption>
                  <NativeSelectOption value="custom">Custom</NativeSelectOption>
                </NativeSelect>
              </Field>
              <Field>
                <FieldLabel htmlFor={patternColorPresetId}>Colour</FieldLabel>
                <NativeSelect
                  id={patternColorPresetId}
                  className="w-full"
                  value={settings.patternColorPreset}
                  data-testid={`${testIdPrefix}-pattern-colour-preset`}
                  onChange={(event) => onSettingsChange({
                    ...settings,
                    patternColorPreset: event.currentTarget.value as BlankPdfPatternColorPreset,
                  })}
                >
                  <NativeSelectOption value="black">Black</NativeSelectOption>
                  <NativeSelectOption value="grey">Grey</NativeSelectOption>
                  <NativeSelectOption value="blue">Light blue</NativeSelectOption>
                  <NativeSelectOption value="custom">Custom</NativeSelectOption>
                </NativeSelect>
              </Field>
            </FieldGroup>

            {settings.patternSpacingPreset === 'custom' ? (
              <DimensionField
                id={customPatternSpacingId}
                label="Custom spacing (mm)"
                value={settings.customPatternSpacing}
                invalid={error !== null}
                min="1"
                max="500"
                testId={`${testIdPrefix}-custom-pattern-spacing`}
                onChange={(customPatternSpacing) => onSettingsChange({ ...settings, customPatternSpacing })}
              />
            ) : null}

            {settings.patternColorPreset === 'custom' ? (
              <Field>
                <FieldLabel htmlFor={customPatternColorId}>Custom colour</FieldLabel>
                <Input
                  id={customPatternColorId}
                  type="color"
                  value={settings.customPatternColor}
                  data-testid={`${testIdPrefix}-custom-pattern-colour`}
                  onChange={(event) => onSettingsChange({ ...settings, customPatternColor: event.currentTarget.value })}
                />
              </Field>
            ) : null}
          </>
        ) : null}
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
  min = '10',
  max = '5000',
  testId,
  onChange,
}: {
  id: string;
  label: string;
  value: string;
  invalid: boolean;
  min?: string;
  max?: string;
  testId: string;
  onChange: (value: string) => void;
}) {
  return (
    <Field data-invalid={invalid || undefined}>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Input
        id={id}
        type="number"
        min={min}
        max={max}
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
