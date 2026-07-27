import { useId, useMemo, useState } from 'react';
import type { KeyboardEvent, ReactNode } from 'react';
import {
  applyPageScale,
  BUILT_IN_SCALE_PRESETS,
  calibratePageScale,
  createCustomPageScale,
  createPresetPageScale,
  deleteUserScalePreset,
  parsePageScaleRanges,
  pdfPoint,
  saveScalePreset,
  scalePresetLabel,
  type DocumentModel,
  type PageScale,
  type PageScaleApplyTarget,
  type PdfPoint,
  type ScalePrecision,
  type ScalePrecisionMode,
  type ScaleUnit,
} from '@butter-paper/core';
import { X } from 'lucide-react';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Field as FormField, FieldLabel, FieldLegend, FieldSet } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';

type ScaleMode = 'preset' | 'custom' | 'calibrate';
type PagesMode = 'current' | 'all' | 'custom';

const UNIT_OPTIONS: readonly ScaleUnit[] = ['in', 'ft', 'mm', 'cm', 'm'];
const DECIMAL_PRECISION_OPTIONS = [1, 0.1, 0.01, 0.001, 0.0001, 0.00001, 0.000001] as const;
const FRACTION_PRECISION_OPTIONS = [2, 4, 8, 16, 32, 64] as const;

interface PageScaleDialogProps {
  document: DocumentModel;
  currentPage: number;
  initialMode?: ScaleMode;
  initialCalibrationPoints?: {
    readonly pageIndex: number;
    readonly start: PdfPoint;
    readonly end: PdfPoint;
  } | null;
  onRequestCalibrationPick: () => void;
  onApply: (updater: (document: DocumentModel) => DocumentModel, message: string) => void;
  onClose: () => void;
}

export function PageScaleDialog({
  document,
  currentPage,
  initialMode = 'preset',
  initialCalibrationPoints = null,
  onRequestCalibrationPick,
  onApply,
  onClose,
}: PageScaleDialogProps) {
  const pageCount = document.pages.length;
  const userPresets = document.scalePresets ?? [];
  const presets = useMemo(() => [...userPresets, ...BUILT_IN_SCALE_PRESETS], [userPresets]);
  const [mode, setMode] = useState<ScaleMode>(initialMode);
  const [pagesMode, setPagesMode] = useState<PagesMode>('current');
  const [customRange, setCustomRange] = useState('');
  const [presetId, setPresetId] = useState(presets[0]?.id ?? '');
  const [pdfLength, setPdfLength] = useState('1');
  const [pdfUnits, setPdfUnits] = useState<ScaleUnit>('cm');
  const [realLength, setRealLength] = useState('1');
  const [realUnits, setRealUnits] = useState<ScaleUnit>('m');
  const [separateYScale, setSeparateYScale] = useState(false);
  const [yPdfLength, setYPdfLength] = useState('1');
  const [yRealLength, setYRealLength] = useState('1');
  const [calibrateStartX, setCalibrateStartX] = useState(initialCalibrationPoints ? formatCoordinate(initialCalibrationPoints.start.x) : '0');
  const [calibrateStartY, setCalibrateStartY] = useState(initialCalibrationPoints ? formatCoordinate(initialCalibrationPoints.start.y) : '0');
  const [calibrateEndX, setCalibrateEndX] = useState(initialCalibrationPoints ? formatCoordinate(initialCalibrationPoints.end.x) : '72');
  const [calibrateEndY, setCalibrateEndY] = useState(initialCalibrationPoints ? formatCoordinate(initialCalibrationPoints.end.y) : '0');
  const [precisionMode, setPrecisionMode] = useState<ScalePrecisionMode>('decimal');
  const [decimalPrecision, setDecimalPrecision] = useState('0.001');
  const [fractionPrecision, setFractionPrecision] = useState('16');
  const [savePreset, setSavePreset] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const precision: ScalePrecision = {
    mode: precisionMode,
    value: Number(precisionMode === 'decimal' ? decimalPrecision : fractionPrecision),
  };

  function handleApply() {
    try {
      const target = buildApplyTarget(pagesMode, currentPage, customRange, pageCount);
      const scale = buildPageScale();
      onApply(
        (currentDocument) => {
          let nextDocument = currentDocument;
          if (savePreset && mode !== 'preset') {
            nextDocument = saveScalePreset(nextDocument, {
              id: `scale-${Date.now().toString(36)}`,
              name: scale.name,
              pdfUnits: scale.pdfUnits,
              realUnits: scale.realUnits,
              scaleX: scale.scaleX,
              scaleY: scale.scaleY,
              source: scale.source,
            });
          }
          return applyPageScale(nextDocument, scale, target);
        },
        `Applied scale to ${targetLabel(target, currentPage, pageCount)}`,
      );
      onClose();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Unable to apply page scale.');
    }
  }

  function buildPageScale(): PageScale {
    const selectedPreset = presets.find((preset) => preset.id === presetId) ?? presets[0];
    if (mode === 'preset') {
      if (!selectedPreset) {
        throw new Error('Select a scale preset.');
      }
      return createPresetPageScale(currentPage, selectedPreset, precision);
    }

    if (mode === 'calibrate') {
      return calibratePageScale({
        pageIndex: currentPage,
        start: pdfPoint(readPositiveOrZero(calibrateStartX, 'Start X'), readPositiveOrZero(calibrateStartY, 'Start Y')),
        end: pdfPoint(readPositiveOrZero(calibrateEndX, 'End X'), readPositiveOrZero(calibrateEndY, 'End Y')),
        realLength: readPositive(realLength, 'Real length'),
        realUnits,
        name: `Calibrated ${realLength} ${realUnits}`,
        precision,
      });
    }

    const parsedPdfLength = readPositive(pdfLength, 'PDF length');
    const parsedRealLength = readPositive(realLength, 'Real length');
    const parsedYPdfLength = separateYScale ? readPositive(yPdfLength, 'Y PDF length') : undefined;
    const parsedYRealLength = separateYScale ? readPositive(yRealLength, 'Y real length') : undefined;
    return createCustomPageScale({
      pageIndex: currentPage,
      name: scalePresetLabel(parsedPdfLength, pdfUnits, parsedRealLength, realUnits),
      pdfUnits,
      realUnits,
      pdfLength: parsedPdfLength,
      realLength: parsedRealLength,
      yPdfLength: parsedYPdfLength,
      yRealLength: parsedYRealLength,
      precision,
    });
  }

  function handleDeletePreset(id: string) {
    try {
      onApply((currentDocument) => deleteUserScalePreset(currentDocument, id), 'Deleted saved scale preset');
      if (presetId === id) {
        setPresetId(BUILT_IN_SCALE_PRESETS[0]?.id ?? '');
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Unable to delete preset.');
    }
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) {
          onClose();
        }
      }}
    >
      <DialogContent className="max-h-[calc(100vh-2rem)] w-[520px] max-w-[calc(100%-2rem)] grid-rows-[auto_minmax(0,1fr)] gap-0 overflow-hidden rounded-2xl p-0 sm:max-w-[520px]" data-testid="page-scale-dialog" finalFocus={() => getPageScaleReturnFocus()} showCloseButton={false}>
        <DialogHeader className="flex-row items-center justify-between gap-4 border-b border-border px-4 py-3">
          <div className="min-w-0">
            <DialogTitle>Set Page Scale</DialogTitle>
            <DialogDescription className="mt-1 text-xs">
              Page {currentPage + 1} of {pageCount}. Choose a scale and the pages it applies to.
            </DialogDescription>
          </div>
          <DialogClose render={<Button variant="ghost" size="icon-sm" aria-label="Close" data-testid="page-scale-close" />}>
            <X aria-hidden="true" />
          </DialogClose>
        </DialogHeader>

        <form
          className="grid min-h-0 grid-rows-[minmax(0,1fr)_auto]"
          onSubmit={(event) => {
            event.preventDefault();
            handleApply();
          }}
        >
          <div className="min-h-0 space-y-4 overflow-y-auto px-4 py-4" data-testid="page-scale-dialog-body">
            <SegmentedControl
              label="Method"
              value={mode}
              options={[
                { value: 'preset', label: 'Preset' },
                { value: 'custom', label: 'Custom' },
                { value: 'calibrate', label: 'Calibrate' },
              ]}
              onChange={(value) => {
                const nextMode = value as ScaleMode;
                if (nextMode === 'calibrate') {
                  onRequestCalibrationPick();
                  return;
                }
                setMode(nextMode);
              }}
            />

            {mode === 'preset' ? (
              <LabeledField label="Scale" controlId="page-scale-preset-select">
                <div className="flex gap-2">
                  <ScaleSelect
                    className="min-w-0 flex-1"
                    value={presetId}
                    options={presets.map((preset) => ({
                      value: preset.id,
                      label: preset.builtIn ? preset.name : `${preset.name} (saved)`,
                    }))}
                    testId="page-scale-preset-select"
                    onChange={setPresetId}
                  />
                  {userPresets.some((preset) => preset.id === presetId && !preset.builtIn) ? (
                    <Button type="button" variant="outline" size="sm" onClick={() => handleDeletePreset(presetId)}>
                      Delete
                    </Button>
                  ) : null}
                </div>
              </LabeledField>
            ) : null}

            {mode === 'custom' ? (
              <div className="space-y-3">
                <ScaleEquation pdfLength={pdfLength} pdfUnits={pdfUnits} realLength={realLength} realUnits={realUnits} onPdfLengthChange={setPdfLength} onPdfUnitsChange={setPdfUnits} onRealLengthChange={setRealLength} onRealUnitsChange={setRealUnits} testIdPrefix="page-scale-custom" />
                <CheckboxField checked={separateYScale} label="Separate Y scale" testId="page-scale-separate-y" onCheckedChange={setSeparateYScale} />
                {separateYScale ? <ScaleEquation label="Y scale" pdfLength={yPdfLength} pdfUnits={pdfUnits} realLength={yRealLength} realUnits={realUnits} onPdfLengthChange={setYPdfLength} onPdfUnitsChange={setPdfUnits} onRealLengthChange={setYRealLength} onRealUnitsChange={setRealUnits} testIdPrefix="page-scale-y-custom" /> : null}
              </div>
            ) : null}

            {mode === 'calibrate' ? (
              <div className="space-y-3">
                <Alert>
                  <AlertTitle>Calibrate from the PDF</AlertTitle>
                  <AlertDescription>Pick two endpoints on the PDF, then enter the known real-world length.</AlertDescription>
                </Alert>
                <Button type="button" onClick={onRequestCalibrationPick} data-testid="page-scale-pick-calibration">
                  Pick Two Points
                </Button>
                <div className="grid grid-cols-2 gap-3">
                  <NumberField label="Start X" value={calibrateStartX} onChange={setCalibrateStartX} testId="page-scale-calibrate-start-x" readOnly />
                  <NumberField label="Start Y" value={calibrateStartY} onChange={setCalibrateStartY} testId="page-scale-calibrate-start-y" readOnly />
                  <NumberField label="End X" value={calibrateEndX} onChange={setCalibrateEndX} testId="page-scale-calibrate-end-x" readOnly />
                  <NumberField label="End Y" value={calibrateEndY} onChange={setCalibrateEndY} testId="page-scale-calibrate-end-y" readOnly />
                  <NumberField label="Known length" value={realLength} onChange={setRealLength} testId="page-scale-calibrate-real-length" />
                  <UnitField label="Units" ariaLabel="Real units" value={realUnits} onChange={setRealUnits} testId="page-scale-calibrate-real-units" />
                </div>
              </div>
            ) : null}

            <div className="grid grid-cols-2 gap-3">
              <LabeledField label="Pages" controlId="page-scale-pages">
                <ScaleSelect
                  value={pagesMode}
                  options={[
                    { value: 'current', label: `Current (${currentPage + 1})` },
                    { value: 'all', label: 'All Pages' },
                    { value: 'custom', label: 'Custom' },
                  ]}
                  testId="page-scale-pages"
                  onChange={(value) => setPagesMode(value as PagesMode)}
                />
              </LabeledField>
              <FormField className="gap-1.5">
                <FieldLabel id="page-scale-precision-label">Precision</FieldLabel>
                <div className="flex gap-2">
                  <ScaleSelect
                    className="min-w-0 flex-1"
                    ariaLabelledBy="page-scale-precision-label"
                    value={precisionMode}
                    options={[
                      { value: 'decimal', label: 'Decimal' },
                      { value: 'fraction', label: 'Fraction' },
                    ]}
                    testId="page-scale-precision-mode"
                    onChange={(value) => setPrecisionMode(value as ScalePrecisionMode)}
                  />
                  <ScaleSelect
                    className="w-24"
                    ariaLabel="Precision value"
                    value={precisionMode === 'decimal' ? decimalPrecision : fractionPrecision}
                    options={(precisionMode === 'decimal' ? DECIMAL_PRECISION_OPTIONS : FRACTION_PRECISION_OPTIONS).map((option) => ({
                      value: String(option),
                      label: precisionMode === 'fraction' ? `1/${option}` : String(option),
                    }))}
                    testId="page-scale-precision-value"
                    onChange={(value) => (precisionMode === 'decimal' ? setDecimalPrecision(value) : setFractionPrecision(value))}
                  />
                </div>
              </FormField>
            </div>

            {pagesMode === 'custom' ? (
              <LabeledField label="Page range" controlId="page-scale-range">
                <Input id="page-scale-range" value={customRange} onChange={(event) => setCustomRange(event.target.value)} placeholder="1-3, 5, 9" data-testid="page-scale-range" />
              </LabeledField>
            ) : null}

            {mode !== 'preset' ? <CheckboxField checked={savePreset} label="Add preset" testId="page-scale-save-preset" onCheckedChange={setSavePreset} /> : null}

            {error ? (
              <Alert variant="destructive" data-testid="page-scale-error">
                <AlertTitle>Unable to apply scale</AlertTitle>
                <AlertDescription>{error}</AlertDescription>
              </Alert>
            ) : null}
          </div>

          <DialogFooter className="border-t border-border px-4 py-3">
            <DialogClose render={<Button type="button" variant="outline" />}>Cancel</DialogClose>
            <Button type="submit" data-testid="page-scale-apply">
              Apply Scale
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function buildApplyTarget(mode: PagesMode, currentPage: number, customRange: string, pageCount: number): PageScaleApplyTarget {
  if (mode === 'all') {
    return { kind: 'all' };
  }
  if (mode === 'custom') {
    return { kind: 'ranges', ranges: parsePageScaleRanges(customRange, pageCount) };
  }
  return { kind: 'current', pageIndex: currentPage };
}

function targetLabel(target: PageScaleApplyTarget, currentPage: number, pageCount: number): string {
  if (target.kind === 'all') {
    return `all ${pageCount} pages`;
  }
  if (target.kind === 'ranges') {
    return 'selected pages';
  }
  return `page ${currentPage + 1}`;
}

function readPositive(value: string, label: string): number {
  const number = Number(value);
  if (!Number.isFinite(number) || number <= 0) {
    throw new Error(`${label} must be a positive number.`);
  }
  return number;
}

function readPositiveOrZero(value: string, label: string): number {
  const number = Number(value);
  if (!Number.isFinite(number) || number < 0) {
    throw new Error(`${label} must be zero or greater.`);
  }
  return number;
}

function getPageScaleReturnFocus(): HTMLElement | null {
  return globalThis.document.querySelector<HTMLElement>('[data-testid="viewer-set-page-scale"]');
}

function LabeledField({ label, controlId, children }: { label: string; controlId: string; children: ReactNode }) {
  return (
    <FormField className="gap-1.5">
      <FieldLabel htmlFor={controlId}>{label}</FieldLabel>
      {children}
    </FormField>
  );
}

function NumberField({
  label,
  value,
  onChange,
  testId,
  readOnly = false,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  testId?: string;
  readOnly?: boolean;
}) {
  const generatedId = useId();
  const controlId = testId ?? generatedId;

  return (
    <LabeledField label={label} controlId={controlId}>
      <Input id={controlId} type="number" value={value} readOnly={readOnly} onChange={(event) => onChange(event.target.value)} data-testid={testId} />
    </LabeledField>
  );
}

function formatCoordinate(value: number): string {
  return Number(value.toFixed(3)).toString();
}

function UnitField({ label, ariaLabel, value, onChange, testId }: { label: string; ariaLabel?: string; value: ScaleUnit; onChange: (value: ScaleUnit) => void; testId?: string }) {
  const generatedId = useId();
  const controlId = testId ?? generatedId;

  return (
    <LabeledField label={label} controlId={controlId}>
      <ScaleSelect value={value} options={UNIT_OPTIONS.map((unit) => ({ value: unit, label: unit }))} testId={testId} id={controlId} ariaLabel={ariaLabel} onChange={onChange} />
    </LabeledField>
  );
}

function ScaleEquation({
  label = 'Scale',
  pdfLength,
  pdfUnits,
  realLength,
  realUnits,
  onPdfLengthChange,
  onPdfUnitsChange,
  onRealLengthChange,
  onRealUnitsChange,
  testIdPrefix,
}: {
  label?: string;
  pdfLength: string;
  pdfUnits: ScaleUnit;
  realLength: string;
  realUnits: ScaleUnit;
  onPdfLengthChange: (value: string) => void;
  onPdfUnitsChange: (value: ScaleUnit) => void;
  onRealLengthChange: (value: string) => void;
  onRealUnitsChange: (value: ScaleUnit) => void;
  testIdPrefix?: string;
}) {
  return (
    <FieldSet className="gap-2">
      <FieldLegend variant="label" className="mb-0 text-muted-foreground">
        {label}
      </FieldLegend>
      <div className="grid grid-cols-[1fr_92px_auto_1fr_92px] items-end gap-2">
        <NumberField label="PDF" value={pdfLength} onChange={onPdfLengthChange} testId={testIdPrefix ? `${testIdPrefix}-pdf-length` : undefined} />
        <UnitField label="Units" ariaLabel="PDF units" value={pdfUnits} onChange={onPdfUnitsChange} testId={testIdPrefix ? `${testIdPrefix}-pdf-units` : undefined} />
        <span className="pb-2 text-[12px]">=</span>
        <NumberField label="Real" value={realLength} onChange={onRealLengthChange} testId={testIdPrefix ? `${testIdPrefix}-real-length` : undefined} />
        <UnitField label="Units" ariaLabel="Real units" value={realUnits} onChange={onRealUnitsChange} testId={testIdPrefix ? `${testIdPrefix}-real-units` : undefined} />
      </div>
    </FieldSet>
  );
}

function SegmentedControl({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: readonly { value: string; label: string }[];
  onChange: (value: string) => void;
}) {
  function handleKeyDown(event: KeyboardEvent): void {
    if (event.altKey || event.ctrlKey || event.metaKey) {
      return;
    }

    const currentIndex = Math.max(0, options.findIndex((option) => option.value === value));
    let nextIndex: number | null = null;
    if (event.key === 'ArrowLeft') {
      nextIndex = (currentIndex - 1 + options.length) % options.length;
    } else if (event.key === 'ArrowRight') {
      nextIndex = (currentIndex + 1) % options.length;
    } else if (event.key === 'Home') {
      nextIndex = 0;
    } else if (event.key === 'End') {
      nextIndex = options.length - 1;
    }

    if (nextIndex !== null) {
      const nextOption = options[nextIndex];
      if (nextOption) {
        onChange(nextOption.value);
      }
    }
  }

  return (
    <FieldSet className="gap-1.5">
      <FieldLegend variant="label" className="mb-0">
        {label}
      </FieldLegend>
      <ToggleGroup
        aria-label={label}
        className="rounded-2xl border border-border"
        spacing={0}
        variant="outline"
        value={[value]}
        onKeyDown={handleKeyDown}
        onValueChange={(values) => {
          const nextValue = values[0];
          if (nextValue) {
            onChange(nextValue);
          }
        }}
      >
        {options.map((option) => (
          <ToggleGroupItem key={option.value} value={option.value} className="px-3" data-testid={`page-scale-method-${option.value}`}>
            {option.label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </FieldSet>
  );
}

function CheckboxField({ checked, label, testId, onCheckedChange }: { checked: boolean; label: string; testId: string; onCheckedChange: (checked: boolean) => void }) {
  return (
    <FormField orientation="horizontal" className="w-fit gap-2">
      <Checkbox id={testId} checked={checked} data-testid={testId} onCheckedChange={onCheckedChange} />
      <FieldLabel htmlFor={testId}>{label}</FieldLabel>
    </FormField>
  );
}

function ScaleSelect<Value extends string>({
  value,
  options,
  onChange,
  testId,
  id = testId,
  className,
  ariaLabel,
  ariaLabelledBy,
}: {
  value: Value;
  options: readonly { value: Value; label: string }[];
  onChange: (value: Value) => void;
  testId?: string;
  id?: string;
  className?: string;
  ariaLabel?: string;
  ariaLabelledBy?: string;
}) {
  return (
    <Select
      items={options}
      value={value}
      onValueChange={(nextValue) => {
        if (nextValue !== null) {
          onChange(nextValue);
        }
      }}
    >
      <SelectTrigger id={id} className={['w-full', className].filter(Boolean).join(' ')} aria-label={ariaLabel} aria-labelledby={ariaLabelledBy} data-testid={testId}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent align="start" alignItemWithTrigger={false}>
        {options.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
