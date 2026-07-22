import { getPageScale, type Markup, type ScaleUnit } from '@butter-paper/core';
import { useViewerStore } from '../state/viewerStore';
import { CustomScrollArea } from './CustomScrollArea';
import {
  SHELL_BORDER_SUBTLE,
  SHELL_TEXT_MUTED,
  SHELL_TEXT_PRIMARY,
} from './shellSpacing';

type MeasurementMarkup = Extract<Markup, { kind: 'length' | 'polylength' | 'area' }>;

const PAGE_SCALE_UNIT = 'page-scale';
const DISPLAY_UNITS: readonly ScaleUnit[] = ['mm', 'cm', 'm', 'in', 'ft'];

export function ToolPropertiesPanel() {
  const documentState = useViewerStore((state) => state.document);
  const selectedMarkupIds = useViewerStore((state) => state.selectedMarkupIds);
  const updateDocument = useViewerStore((state) => state.updateDocument);

  const document = documentState?.document;
  const selectedMarkup = document && selectedMarkupIds.length === 1
    ? document.markups.find((markup) => markup.id === selectedMarkupIds[0])
    : undefined;
  const measurementMarkup = selectedMarkup && isMeasurementMarkup(selectedMarkup) ? selectedMarkup : undefined;
  const pageScale = measurementMarkup && document ? getPageScale(document, measurementMarkup.pageIndex) : undefined;

  function handleDisplayUnitChange(value: string): void {
    if (!measurementMarkup) {
      return;
    }
    const displayUnit = value === PAGE_SCALE_UNIT ? undefined : value as ScaleUnit;
    updateDocument((currentDocument) => ({
      ...currentDocument,
      markups: currentDocument.markups.map((markup) => (
        markup.id === measurementMarkup.id && isMeasurementMarkup(markup)
          ? { ...markup, displayUnit }
          : markup
      )),
    }));
  }

  return (
    <CustomScrollArea
      className="min-h-0 flex-1"
      viewportClassName="overflow-y-auto overflow-x-hidden"
      viewportTestId="tool-properties-panel"
      verticalTrackTestId="tool-properties-scrollbar-track"
      verticalThumbTestId="tool-properties-scrollbar-thumb"
    >
      <div className="min-h-full px-3 py-3">
        {measurementMarkup ? (
          <section className="space-y-3" data-testid="measurement-properties-panel">
            <div>
              <div className={['text-[12px] font-semibold', SHELL_TEXT_PRIMARY].join(' ')}>Measurement</div>
              <div className={['mt-0.5 text-[11px]', SHELL_TEXT_MUTED].join(' ')}>
                {measurementKindLabel(measurementMarkup.kind)}
              </div>
            </div>
            <label className="block">
              <span className={['mb-1 block text-[11px] font-medium', SHELL_TEXT_MUTED].join(' ')}>Units</span>
              <select
                value={measurementMarkup.displayUnit ?? PAGE_SCALE_UNIT}
                onChange={(event) => handleDisplayUnitChange(event.target.value)}
                className={[
                  'h-8 w-full rounded-[6px] border bg-transparent px-2 text-[12px] outline-none',
                  SHELL_BORDER_SUBTLE,
                  SHELL_TEXT_PRIMARY,
                ].join(' ')}
                data-testid="measurement-display-unit"
              >
                <option value={PAGE_SCALE_UNIT}>Page scale{pageScale ? ` (${pageScale.realUnits})` : ''}</option>
                {DISPLAY_UNITS.map((unit) => (
                  <option key={unit} value={unit}>{unit}</option>
                ))}
              </select>
            </label>
          </section>
        ) : null}
      </div>
    </CustomScrollArea>
  );
}

function isMeasurementMarkup(markup: Markup): markup is MeasurementMarkup {
  return markup.kind === 'length' || markup.kind === 'polylength' || markup.kind === 'area';
}

function measurementKindLabel(kind: MeasurementMarkup['kind']): string {
  switch (kind) {
    case 'length':
      return 'Length';
    case 'polylength':
      return 'Polylength';
    case 'area':
      return 'Area';
    default:
      return 'Measurement';
  }
}
