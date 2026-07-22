import { useMemo, useState } from 'react';
import type { ReactNode } from 'react';
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
  type ScalePreset,
  type ScaleUnit,
} from '@butter-paper/core';
import { X } from 'lucide-react';
import {
  CONTROL_ACTIVE,
  CONTROL_DEFAULT,
  CONTROL_ICON_SIZE,
  CONTROL_ICON_STROKE_WIDTH,
  SHELL_BORDER_SUBTLE,
  SHELL_CONTROL_GAP,
  SHELL_SURFACE_PANEL,
  SHELL_TEXT_MUTED,
  SHELL_TEXT_PRIMARY,
} from './shellSpacing';

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
      onApply((currentDocument) => {
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
      }, `Applied scale to ${targetLabel(target, currentPage, pageCount)}`);
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
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/35 pt-20" data-testid="page-scale-dialog">
      <section className={['w-[520px] rounded-[8px] border shadow-xl', SHELL_SURFACE_PANEL, SHELL_BORDER_SUBTLE, SHELL_TEXT_PRIMARY].join(' ')}>
        <header className={['flex items-center justify-between border-b px-4 py-3', SHELL_BORDER_SUBTLE].join(' ')}>
          <div>
            <h2 className="text-[14px] font-semibold">Set Page Scale</h2>
            <p className={['mt-0.5 text-[11px]', SHELL_TEXT_MUTED].join(' ')}>Page {currentPage + 1} of {pageCount}</p>
          </div>
          <button type="button" aria-label="Close" className={['rounded-[6px] border p-1.5', CONTROL_DEFAULT].join(' ')} onClick={onClose}>
            <X size={CONTROL_ICON_SIZE} strokeWidth={CONTROL_ICON_STROKE_WIDTH} aria-hidden="true" />
          </button>
        </header>

        <div className="space-y-4 px-4 py-4">
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
            <Field label="Scale">
              <div className="flex gap-2">
                <select value={presetId} onChange={(event) => setPresetId(event.target.value)} className="min-w-0 flex-1 rounded-[6px] border bg-transparent px-2 py-1.5 text-[12px]" data-testid="page-scale-preset-select">
                  {presets.map((preset) => (
                    <option key={preset.id} value={preset.id}>
                      {preset.builtIn ? preset.name : `${preset.name} (saved)`}
                    </option>
                  ))}
                </select>
                {userPresets.some((preset) => preset.id === presetId && !preset.builtIn) ? (
                  <button type="button" className={['rounded-[6px] border px-2 text-[12px]', CONTROL_DEFAULT].join(' ')} onClick={() => handleDeletePreset(presetId)}>
                    Delete
                  </button>
                ) : null}
              </div>
            </Field>
          ) : null}

          {mode === 'custom' ? (
            <div className="space-y-3">
              <ScaleEquation
                pdfLength={pdfLength}
                pdfUnits={pdfUnits}
                realLength={realLength}
                realUnits={realUnits}
                onPdfLengthChange={setPdfLength}
                onPdfUnitsChange={setPdfUnits}
                onRealLengthChange={setRealLength}
                onRealUnitsChange={setRealUnits}
                testIdPrefix="page-scale-custom"
              />
              <label className="flex items-center gap-2 text-[12px]">
                <input type="checkbox" checked={separateYScale} onChange={(event) => setSeparateYScale(event.target.checked)} />
                Separate Y scale
              </label>
              {separateYScale ? (
                <ScaleEquation
                  label="Y scale"
                  pdfLength={yPdfLength}
                  pdfUnits={pdfUnits}
                  realLength={yRealLength}
                  realUnits={realUnits}
                  onPdfLengthChange={setYPdfLength}
                  onPdfUnitsChange={setPdfUnits}
                  onRealLengthChange={setYRealLength}
                  onRealUnitsChange={setRealUnits}
                  testIdPrefix="page-scale-y-custom"
                />
              ) : null}
            </div>
          ) : null}

          {mode === 'calibrate' ? (
            <div className="space-y-3">
              <div className="rounded-[6px] border border-blue-200 bg-blue-50 px-3 py-2 text-[12px] text-blue-800">
                Click Calibrate to pick two endpoints on the PDF, then enter the known real-world length.
              </div>
              <button type="button" className={['rounded-[6px] border px-3 py-1.5 text-[12px]', CONTROL_ACTIVE].join(' ')} onClick={onRequestCalibrationPick} data-testid="page-scale-pick-calibration">
                Pick Two Points
              </button>
              <div className="grid grid-cols-2 gap-3">
                <NumberField label="Start X" value={calibrateStartX} onChange={setCalibrateStartX} testId="page-scale-calibrate-start-x" readOnly />
                <NumberField label="Start Y" value={calibrateStartY} onChange={setCalibrateStartY} testId="page-scale-calibrate-start-y" readOnly />
                <NumberField label="End X" value={calibrateEndX} onChange={setCalibrateEndX} testId="page-scale-calibrate-end-x" readOnly />
                <NumberField label="End Y" value={calibrateEndY} onChange={setCalibrateEndY} testId="page-scale-calibrate-end-y" readOnly />
                <NumberField label="Known length" value={realLength} onChange={setRealLength} testId="page-scale-calibrate-real-length" />
                <UnitField label="Units" value={realUnits} onChange={setRealUnits} testId="page-scale-calibrate-real-units" />
              </div>
            </div>
          ) : null}

          <div className="grid grid-cols-2 gap-3">
            <Field label="Pages">
              <select value={pagesMode} onChange={(event) => setPagesMode(event.target.value as PagesMode)} className="w-full rounded-[6px] border bg-transparent px-2 py-1.5 text-[12px]" data-testid="page-scale-pages">
                <option value="current">Current ({currentPage + 1})</option>
                <option value="all">All Pages</option>
                <option value="custom">Custom</option>
              </select>
            </Field>
            <Field label="Precision">
              <div className="flex gap-2">
                <select value={precisionMode} onChange={(event) => setPrecisionMode(event.target.value as ScalePrecisionMode)} className="min-w-0 flex-1 rounded-[6px] border bg-transparent px-2 py-1.5 text-[12px]">
                  <option value="decimal">Decimal</option>
                  <option value="fraction">Fraction</option>
                </select>
                <select value={precisionMode === 'decimal' ? decimalPrecision : fractionPrecision} onChange={(event) => precisionMode === 'decimal' ? setDecimalPrecision(event.target.value) : setFractionPrecision(event.target.value)} className="w-24 rounded-[6px] border bg-transparent px-2 py-1.5 text-[12px]">
                  {(precisionMode === 'decimal' ? DECIMAL_PRECISION_OPTIONS : FRACTION_PRECISION_OPTIONS).map((option) => (
                    <option key={option} value={option}>{precisionMode === 'fraction' ? `1/${option}` : option}</option>
                  ))}
                </select>
              </div>
            </Field>
          </div>

          {pagesMode === 'custom' ? (
            <Field label="Page range">
              <input value={customRange} onChange={(event) => setCustomRange(event.target.value)} placeholder="1-3, 5, 9" className="w-full rounded-[6px] border bg-transparent px-2 py-1.5 text-[12px]" data-testid="page-scale-range" />
            </Field>
          ) : null}

          {mode !== 'preset' ? (
            <label className="flex items-center gap-2 text-[12px]">
              <input type="checkbox" checked={savePreset} onChange={(event) => setSavePreset(event.target.checked)} data-testid="page-scale-save-preset" />
              Add preset
            </label>
          ) : null}

          {error ? <div className="rounded-[6px] border border-red-200 bg-red-50 px-3 py-2 text-[12px] text-red-700">{error}</div> : null}
        </div>

        <footer className={['flex items-center justify-end border-t px-4 py-3', SHELL_BORDER_SUBTLE, SHELL_CONTROL_GAP].join(' ')}>
          <button type="button" className={['rounded-[6px] border px-3 py-1.5 text-[12px]', CONTROL_DEFAULT].join(' ')} onClick={onClose}>
            Cancel
          </button>
          <button type="button" className={['rounded-[6px] border px-3 py-1.5 text-[12px]', CONTROL_ACTIVE].join(' ')} onClick={handleApply} data-testid="page-scale-apply">
            Apply Scale
          </button>
        </footer>
      </section>
    </div>
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

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block text-[12px]">
      <span className={['mb-1 block', SHELL_TEXT_MUTED].join(' ')}>{label}</span>
      {children}
    </label>
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
  return (
    <Field label={label}>
      <input type="number" value={value} readOnly={readOnly} onChange={(event) => onChange(event.target.value)} className="w-full rounded-[6px] border bg-transparent px-2 py-1.5 text-[12px]" data-testid={testId} />
    </Field>
  );
}

function formatCoordinate(value: number): string {
  return Number(value.toFixed(3)).toString();
}

function UnitField({ label, value, onChange, testId }: { label: string; value: ScaleUnit; onChange: (value: ScaleUnit) => void; testId?: string }) {
  return (
    <Field label={label}>
      <select value={value} onChange={(event) => onChange(event.target.value as ScaleUnit)} className="w-full rounded-[6px] border bg-transparent px-2 py-1.5 text-[12px]" data-testid={testId}>
        {UNIT_OPTIONS.map((unit) => <option key={unit} value={unit}>{unit}</option>)}
      </select>
    </Field>
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
    <div>
      <div className={['mb-1 text-[12px]', SHELL_TEXT_MUTED].join(' ')}>{label}</div>
      <div className="grid grid-cols-[1fr_92px_auto_1fr_92px] items-end gap-2">
        <NumberField label="PDF" value={pdfLength} onChange={onPdfLengthChange} testId={testIdPrefix ? `${testIdPrefix}-pdf-length` : undefined} />
        <UnitField label="Units" value={pdfUnits} onChange={onPdfUnitsChange} testId={testIdPrefix ? `${testIdPrefix}-pdf-units` : undefined} />
        <span className="pb-2 text-[12px]">=</span>
        <NumberField label="Real" value={realLength} onChange={onRealLengthChange} testId={testIdPrefix ? `${testIdPrefix}-real-length` : undefined} />
        <UnitField label="Units" value={realUnits} onChange={onRealUnitsChange} testId={testIdPrefix ? `${testIdPrefix}-real-units` : undefined} />
      </div>
    </div>
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
  return (
    <div>
      <div className={['mb-1 text-[12px]', SHELL_TEXT_MUTED].join(' ')}>{label}</div>
      <div className="inline-flex rounded-[6px] border p-0.5">
        {options.map((option) => (
          <button
            key={option.value}
            type="button"
            className={[
              'rounded-[4px] px-3 py-1.5 text-[12px]',
              value === option.value ? CONTROL_ACTIVE : 'hover:bg-neutral-100 dark:hover:bg-neutral-800',
            ].join(' ')}
            data-testid={`page-scale-method-${option.value}`}
            onClick={() => onChange(option.value)}
          >
            {option.label}
          </button>
        ))}
      </div>
    </div>
  );
}
