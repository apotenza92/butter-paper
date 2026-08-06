import { readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const PROPERTY_CONTRACT_SCHEMA = 'butter-paper/bluebeam-property-contract';
export const PROPERTY_CONTRACT_VERSION = 1;
export const PROPERTY_STATUSES = ['exact', 'preserved-untouched', 'normalized', 'visual-only', 'unsupported', 'unverified'];

const manual = 'https://support.bluebeam.com/user-manual/dashboard.html';
const propertiesPanel = 'https://support.bluebeam.com/user-manual/menus/window/file-properties.html';
const propertiesToolbar = 'https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html';

export const sourceCatalog = {
  manual: { title: 'Revu 21 user manual', url: manual },
  propertiesPanel: { title: 'Properties panel', url: propertiesPanel },
  propertiesToolbar: { title: 'Properties toolbar', url: propertiesToolbar },
  rectangle: { title: 'Rectangle tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/rectangle-tool.html' },
  ellipse: { title: 'Ellipse tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/ellipse-tool.html' },
  line: { title: 'Line tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/line-tool.html' },
  arc: { title: 'Arc tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/arc-tool.html' },
  polyline: { title: 'Polyline tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/polyline-tool.html' },
  polygon: { title: 'Polygon tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/polygon-tool.html' },
  pen: { title: 'Pen tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/pen-tool.html' },
  highlight: { title: 'Highlight tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/highlight-tool.html' },
  cloud: { title: 'Cloud tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/cloud-tool.html' },
  callout: { title: 'Callout tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/callout-tool.html' },
  textBox: { title: 'Text Box tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/text-box-tool.html' },
  image: { title: 'Image tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/image-tool.html' },
  snapshot: { title: 'Snapshot tool', url: 'https://support.bluebeam.com/user-manual/menus/edit/snapshot.html' },
  dimension: { title: 'Dimension tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/dimension-tool.html' },
  length: { title: 'Length measurement', url: 'https://support.bluebeam.com/user-manual/menus/tools/length-measurement.html' },
  polylength: { title: 'Polylength measurement', url: 'https://support.bluebeam.com/user-manual/menus/tools/polylength-measurement.html' },
  area: { title: 'Area measurement', url: 'https://support.bluebeam.com/user-manual/menus/tools/area-measurement.html' },
  count: { title: 'Count tool', url: 'https://support.bluebeam.com/user-manual/menus/tools/count-tool.html' },
  lineStyles: { title: 'Custom line styles', url: 'https://support.bluebeam.com/user-manual/toolbars/custom-line-styles.html' },
};

export const evidenceCatalog = {
  revu212Environment: {
    kind: 'revu-vm-manifest',
    application: 'Bluebeam Revu',
    version: '21.2.0.1883+a60e43309c872115ec97fb2473da8ede83f263a3',
    os: 'Windows 11',
    artifact: 'test-results/bluebeam-compat/all-tools-revu-final-contract-2026-08-04/revu-manifest.json',
    scope: 'Confirms the installed application and capture environment; it does not by itself prove an individual property.',
  },
  documentationOnly: {
    kind: 'documentation-only',
    scope: 'The property is documented, but no one-variable-at-a-time Revu 21.2 PDF probe is currently attached. Compatibility is unverified.',
  },
  butterUntouchedPolicy: {
    kind: 'implementation-contract',
    artifact: 'scripts/bluebeam-compat/tool-contract.json',
    scope: 'The tested implementation contract preserves untouched imported native components verbatim. It does not prove edited property round trips.',
  },
};

const status = (revuToButter = 'unverified', butterToRevu = 'unverified') => ({ revuToButter, butterToRevu });
const pdf = (standardKeys = [], unverifiedBluebeamKeyCandidates = [], appearanceStream = 'unverified', groupedAnnotations = 'not-applicable') => ({
  standardKeys,
  bluebeamKeys: [],
  unverifiedBluebeamKeyCandidates,
  appearanceStream,
  groupedAnnotations,
});
const p = (key, section, label, meaning, valueType, values, defaultValue, affectedVisualComponent, extra = {}) => ({
  key, section, label, meaning, valueType, values, default: defaultValue, conditions: [], affectedVisualComponent,
  sourceRefs: ['propertiesPanel', 'propertiesToolbar'], evidenceRefs: ['revu212Environment', 'documentationOnly', 'butterUntouchedPolicy'],
  pdf: pdf(), compatibility: status(), primitive: valueType, ...extra,
});

const definitions = {
  subject: p('general.subject', 'General', 'Subject', 'Human-readable markup type or purpose shown in the Markups List.', 'text', null, 'tool label', 'metadata', { pdf: pdf(['/Subj']), compatibility: status('preserved-untouched', 'exact'), primitive: 'text' }),
  author: p('general.author', 'General', 'Author', 'Person attributed as the markup creator.', 'text', null, 'current Revu user', 'metadata', { pdf: pdf(['/T']), compatibility: status('preserved-untouched', 'exact'), primitive: 'text' }),
  label: p('general.label', 'General', 'Label', 'Display label used by the Markups List and summaries.', 'text', null, '', 'metadata', { pdf: pdf(['/Contents', '/Subj'], ['/BSIColumnData']), compatibility: status('preserved-untouched', 'unverified'), primitive: 'text' }),
  layer: p('general.layer', 'General', 'Layer', 'Assigns the markup to a PDF layer.', 'select', 'document layers', 'none', 'visibility/metadata', { pdf: pdf(['/OC']), compatibility: status('preserved-untouched', 'unsupported'), primitive: 'select' }),
  comments: p('general.comments', 'General', 'Comments', 'Free-form markup comment or text content.', 'multiline-text', null, '', 'text/metadata', { pdf: pdf(['/Contents', '/RC']), compatibility: status('preserved-untouched', 'exact'), primitive: 'textarea' }),
  locked: p('general.locked', 'General', 'Locked', 'Prevents interactive changes until unlocked.', 'boolean', [false, true], false, 'interaction', { pdf: pdf(['/F']), compatibility: status('preserved-untouched', 'exact'), primitive: 'switch' }),
  printable: p('general.printable', 'General', 'Print', 'Controls whether the markup is included when printing.', 'boolean', [false, true], true, 'print visibility', { pdf: pdf(['/F']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'switch' }),
  status: p('general.status', 'General', 'Status', 'Review workflow state applied to the markup.', 'select', 'model-specific status values', 'none', 'metadata', { pdf: pdf(['/State', '/StateModel']), compatibility: status('preserved-untouched', 'unsupported'), primitive: 'select' }),
  strokeColor: p('appearance.stroke-color', 'Appearance', 'Color', 'Color of the outline, path, leader, or measurement line.', 'color', 'sRGB/custom/transparent', '#000000', 'stroke', { pdf: pdf(['/C']), compatibility: status('preserved-untouched', 'exact'), primitive: 'color' }),
  fillColor: p('appearance.fill-color', 'Appearance', 'Fill Color', 'Color inside a closed shape or text container.', 'color', 'sRGB/custom/transparent', 'transparent', 'fill', { pdf: pdf(['/IC']), compatibility: status('preserved-untouched', 'exact'), primitive: 'color' }),
  textColor: p('appearance.text-color', 'Appearance', 'Text Color', 'Color of rendered text.', 'color', 'sRGB/custom', '#000000', 'text', { pdf: pdf(['/DA', '/DS']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'color' }),
  opacity: p('appearance.opacity', 'Appearance', 'Opacity', 'Transparency of the stroke and primary visual content.', 'percentage', { min: 0, max: 100, step: 1, unit: '%' }, 100, 'stroke/content', { pdf: pdf(['/CA']), compatibility: status('preserved-untouched', 'exact'), primitive: 'numeric-slider' }),
  fillOpacity: p('appearance.fill-opacity', 'Appearance', 'Fill Opacity', 'Transparency of the interior fill independently of the outline.', 'percentage', { min: 0, max: 100, step: 1, unit: '%' }, 100, 'fill', { pdf: pdf(['/ca'], ['/FillOpacity']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'numeric-slider' }),
  highlight: p('appearance.highlight', 'Appearance', 'Highlight', 'Uses a highlight/blend treatment so underlying page content remains visible.', 'boolean', [false, true], false, 'compositing', { pdf: pdf(['/BM']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'switch' }),
  lineWidth: p('appearance.line-width', 'Appearance', 'Line Width', 'Thickness of the outline or path.', 'number', { min: 0, max: 20, step: 0.25, unit: 'pt' }, 1, 'stroke', { pdf: pdf(['/BS/W', '/Border/2']), compatibility: status('preserved-untouched', 'exact'), primitive: 'numeric-slider' }),
  lineStyle: p('appearance.line-style', 'Appearance', 'Line Style', 'Solid, dashed, or named/custom pattern used for the stroke.', 'select', 'solid and Revu line-style library', 'solid', 'stroke', { sourceRefs: ['propertiesPanel', 'propertiesToolbar', 'lineStyles'], pdf: pdf(['/BS/S', '/BS/D'], ['/BSILineStyle']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'line-style' }),
  hatchPattern: p('appearance.hatch-pattern', 'Appearance', 'Hatch Pattern', 'Pattern used instead of a solid interior fill.', 'select', 'Revu hatch-pattern library', 'none', 'fill', { conditions: [{ property: 'appearance.fill-color', operator: 'is-not', value: 'transparent' }], pdf: pdf([], ['/BSIHatchPattern']), compatibility: status('preserved-untouched', 'visual-only'), primitive: 'hatch' }),
  hatchColor: p('appearance.hatch-color', 'Appearance', 'Hatch Color', 'Color of hatch lines.', 'color', 'sRGB/custom', '#000000', 'hatch', { conditions: [{ property: 'appearance.hatch-pattern', operator: 'is-not', value: 'none' }], pdf: pdf([], ['/BSIHatchColor']), compatibility: status('preserved-untouched', 'visual-only'), primitive: 'color' }),
  hatchScale: p('appearance.hatch-scale', 'Appearance', 'Hatch Scale', 'Scales hatch spacing and pattern size.', 'percentage', { min: 50, max: 200, step: 1, unit: '%' }, 100, 'hatch', { sourceRefs: ['rectangle'], conditions: [{ property: 'appearance.hatch-pattern', operator: 'is-not', value: 'none' }], pdf: pdf([], ['/BSIHatchScale'], 'required for portable visual fidelity'), compatibility: status('preserved-untouched', 'visual-only'), primitive: 'numeric-slider' }),
  startEndpoint: p('appearance.start-endpoint', 'Appearance', 'Start', 'Marker drawn at the start of a line or leader.', 'select', 'Revu endpoint library', 'none', 'start endpoint', { sourceRefs: ['line'], pdf: pdf(['/LE/0'], ['/BSIStart']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'endpoint' }),
  endEndpoint: p('appearance.end-endpoint', 'Appearance', 'End', 'Marker drawn at the end of a line or leader.', 'select', 'Revu endpoint library', 'none', 'end endpoint', { sourceRefs: ['line'], pdf: pdf(['/LE/1'], ['/BSIEnd']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'endpoint' }),
  endpointScale: p('appearance.endpoint-scale', 'Appearance', 'Scale', 'Scales endpoint markers relative to line width; Auto follows Revu sizing.', 'number-or-auto', { min: 0, max: 500, step: 1, unit: '%', special: 'auto' }, 'auto', 'endpoints', { sourceRefs: ['line'], conditions: [{ property: 'appearance.start-endpoint|appearance.end-endpoint', operator: 'any-is-not', value: 'none' }], pdf: pdf([], ['/BSILineEndScale']), compatibility: status('preserved-untouched', 'visual-only'), primitive: 'endpoint-scale' }),
  endpointFill: p('appearance.endpoint-fill', 'Appearance', 'Fill', 'Fills endpoint markers that support open/filled variants.', 'boolean', [false, true], true, 'endpoints', { sourceRefs: ['line'], conditions: [{ property: 'appearance.start-endpoint|appearance.end-endpoint', operator: 'supports-fill', value: true }], pdf: pdf(['/LE'], ['/BSILineEndFill']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'switch' }),
  fontFamily: p('appearance.font-family', 'Appearance', 'Font', 'Typeface used for text.', 'font', 'installed fonts', 'Helvetica', 'text', { pdf: pdf(['/DA', '/DR/Font']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'searchable-font' }),
  fontSize: p('appearance.font-size', 'Appearance', 'Font Size', 'Text size in points.', 'number', { min: 1, max: 144, step: 1, unit: 'pt' }, 12, 'text', { pdf: pdf(['/DA']), compatibility: status('preserved-untouched', 'exact'), primitive: 'numeric' }),
  autoSize: p('appearance.auto-size', 'Appearance', 'Auto Size', 'Automatically sizes the text container or font to fit content.', 'boolean', [false, true], false, 'text/container', { conditions: [], pdf: pdf(['/DA'], ['/BSIAutoSize'], 'typically required'), compatibility: status('preserved-untouched', 'visual-only'), primitive: 'switch' }),
  lineSpacing: p('appearance.line-spacing', 'Appearance', 'Line Spacing', 'Vertical spacing between text baselines.', 'number', { min: 0.5, max: 3, step: 0.1, unit: '×' }, 1, 'text', { pdf: pdf(['/DS'], ['/BSILineSpacing']), compatibility: status('preserved-untouched', 'visual-only'), primitive: 'numeric' }),
  margin: p('appearance.margin', 'Appearance', 'Margin', 'Inset between text and its container border.', 'number', { min: 0, max: 72, step: 0.5, unit: 'pt' }, 3, 'text/container', { pdf: pdf(['/RD'], ['/BSIMargin']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'numeric' }),
  textAlign: p('appearance.text-align', 'Appearance', 'Alignment', 'Horizontal and vertical alignment of text within its box.', 'toggle-group', ['left', 'center', 'right', 'top', 'middle', 'bottom'], 'left/top', 'text', { pdf: pdf(['/Q'], ['/BSIVerticalAlignment']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'finite-toggle' }),
  fontStyles: p('appearance.font-styles', 'Appearance', 'Font Style', 'Bold, italic, underline, strikethrough, superscript, and subscript text emphasis.', 'multi-toggle', ['bold', 'italic', 'underline', 'strikethrough', 'superscript', 'subscript'], [], 'text', { pdf: pdf(['/DA', '/DS'], ['/BSIFontStyle']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'font-style' }),
  cloudSize: p('appearance.cloud-size', 'Appearance', 'Cloud Size', 'Controls the radius/frequency of cloud scallops.', 'number', { min: 0.5, max: 10, step: 0.5, unit: 'pt' }, 2, 'cloud border', { pdf: pdf(['/BE/I'], ['/BSICloudSize']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'numeric-slider' }),
  invertCloud: p('appearance.invert-cloud', 'Appearance', 'Invert', 'Flips cloud scallops inward.', 'boolean', [false, true], false, 'cloud border', { conditions: [{ property: 'appearance.line-style', operator: 'is', value: 'cloud' }], pdf: pdf([], ['/BSICloudInverted'], 'required'), compatibility: status('preserved-untouched', 'visual-only'), primitive: 'switch' }),
  shape: p('appearance.shape', 'Appearance', 'Shape', 'Chooses the text-box container geometry.', 'select', ['rectangle', 'circle', 'triangle', 'hexagon'], 'rectangle', 'container', { sourceRefs: ['callout'], pdf: pdf(['/Subtype', '/RD'], ['/BSIShape']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'select' }),
  units: p('measurement.units', 'Measurement', 'Units', 'Display unit for measured values.', 'select', ['page-scale', 'mm', 'cm', 'm', 'in', 'ft'], 'page-scale', 'measurement caption', { pdf: pdf(['/Measure/U']), compatibility: status('preserved-untouched', 'exact'), primitive: 'units' }),
  precision: p('measurement.precision', 'Measurement', 'Precision', 'Number and fractional precision used for the displayed result.', 'select', 'unit-dependent decimal or fractional precision', 'document scale', 'measurement caption', { pdf: pdf(['/Measure/D']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'precision' }),
  scale: p('measurement.scale', 'Measurement', 'Scale', 'Ratio converting page-space distance into real-world distance.', 'scale', 'page scale or independent scale', 'page scale', 'measurement value', { pdf: pdf(['/Measure/R', '/Measure/X']), compatibility: status('preserved-untouched', 'exact'), primitive: 'scale' }),
  caption: p('measurement.caption', 'Measurement', 'Show Caption', 'Shows the measurement value/caption on the markup.', 'boolean', [false, true], true, 'measurement caption', { pdf: pdf(['/Contents'], ['/BSICaption']), compatibility: status('preserved-untouched', 'normalized'), primitive: 'switch' }),
  captionPosition: p('measurement.caption-position', 'Measurement', 'Caption Position', 'Places the measurement caption along or around the geometry.', 'select', ['inline', 'top', 'bottom'], 'inline', 'measurement caption', { conditions: [{ property: 'measurement.caption', operator: 'is', value: true }], pdf: pdf([], ['/BSICaptionPosition']), compatibility: status('preserved-untouched', 'visual-only'), primitive: 'select' }),
  depth: p('measurement.depth', 'Measurement', 'Depth', 'Depth used to derive volume from an area measurement.', 'number', { min: 0, max: null, step: 0.01, unit: 'measurement units' }, 0, 'calculated value', { conditions: [{ property: 'measurement.mode', operator: 'is', value: 'volume' }], pdf: pdf(['/Measure']), compatibility: status('preserved-untouched', 'unsupported'), primitive: 'numeric' }),
  slope: p('measurement.slope', 'Measurement', 'Slope', 'Applies rise/run or pitch adjustment to a length.', 'number', { min: 0, max: null, step: 0.01, unit: 'ratio' }, 0, 'calculated value', { pdf: pdf(['/Measure'], ['/BSISlope']), compatibility: status('preserved-untouched', 'unsupported'), primitive: 'numeric' }),
  countSymbol: p('count.symbol', 'Measurement', 'Symbol', 'Shape used for each Count instance.', 'select', 'Revu Count symbol library', 'circle', 'count instances', { sourceRefs: ['count'], pdf: pdf(['/AP'], ['/BSICountSymbol'], 'required', 'instances are associated into a Count group'), compatibility: status('unsupported', 'unsupported'), primitive: 'count-symbol' }),
  countScale: p('count.scale', 'Measurement', 'Scale', 'Uniformly scales Count symbols.', 'percentage', { min: 1, max: 500, step: 1, unit: '%' }, 100, 'count instances', { sourceRefs: ['count'], pdf: pdf(['/Rect', '/AP'], ['/BSICountScale'], 'required', 'applies to every instance in the group'), compatibility: status('unsupported', 'unsupported'), primitive: 'numeric-slider' }),
  countWidth: p('count.width', 'Layout', 'Width', 'Width of every Count symbol.', 'number', { min: 0, max: null, step: 0.01, unit: 'page units' }, 'symbol default', 'count instances', { sourceRefs: ['count'], pdf: pdf(['/Rect'], ['/BSICountWidth'], 'required', 'applies to the group'), compatibility: status('unsupported', 'unsupported'), primitive: 'numeric' }),
  countHeight: p('count.height', 'Layout', 'Height', 'Height of every Count symbol.', 'number', { min: 0, max: null, step: 0.01, unit: 'page units' }, 'symbol default', 'count instances', { sourceRefs: ['count'], pdf: pdf(['/Rect'], ['/BSICountHeight'], 'required', 'applies to the group'), compatibility: status('unsupported', 'unsupported'), primitive: 'numeric' }),
  countDepth: p('count.depth', 'Measurement', 'Depth', 'Depth associated with every uniformly dimensioned Count instance.', 'number', { min: 0, max: null, step: 0.01, unit: 'measurement units' }, 0, 'count measurement totals', { sourceRefs: ['count'], pdf: pdf(['/Measure'], ['/BSICountDepth'], 'unverified', 'applies to the group'), compatibility: status('unsupported', 'unsupported'), primitive: 'numeric' }),
  countGroup: p('count.group', 'Measurement', 'Group', 'Logical Count measurement that owns all placed instances and a shared total.', 'group', 'Revu Count group', 'new group', 'all count instances/caption', { sourceRefs: ['count'], pdf: pdf([], ['/BSIGroup', '/BSICount'], 'required', 'fundamental'), compatibility: status('unsupported', 'unsupported'), primitive: 'group' }),
  x: p('layout.x', 'Layout', 'X', 'Horizontal position in page coordinates.', 'number', { step: 0.01, unit: 'page units' }, 0, 'geometry', { pdf: pdf(['/Rect', '/L', '/Vertices']), compatibility: status('preserved-untouched', 'exact'), primitive: 'numeric' }),
  y: p('layout.y', 'Layout', 'Y', 'Vertical position in page coordinates.', 'number', { step: 0.01, unit: 'page units' }, 0, 'geometry', { pdf: pdf(['/Rect', '/L', '/Vertices']), compatibility: status('preserved-untouched', 'exact'), primitive: 'numeric' }),
  width: p('layout.width', 'Layout', 'Width', 'Horizontal extent of the markup bounding box.', 'number', { min: 0, step: 0.01, unit: 'page units' }, 'drawn size', 'geometry', { pdf: pdf(['/Rect']), compatibility: status('preserved-untouched', 'exact'), primitive: 'numeric' }),
  height: p('layout.height', 'Layout', 'Height', 'Vertical extent of the markup bounding box.', 'number', { min: 0, step: 0.01, unit: 'page units' }, 'drawn size', 'geometry', { pdf: pdf(['/Rect']), compatibility: status('preserved-untouched', 'exact'), primitive: 'numeric' }),
  rotation: p('layout.rotation', 'Layout', 'Rotation', 'Clockwise markup rotation.', 'angle', { min: 0, max: 359, step: 1, unit: '°' }, 0, 'geometry/content', { pdf: pdf(['/Rotate'], ['/BSIRotation'], 'may carry rotation visually'), compatibility: status('preserved-untouched', 'normalized'), primitive: 'numeric' }),
  setDefault: p('options.set-default', 'Options', 'Set as Default', 'Uses the current appearance for subsequently created markups of this tool.', 'action', null, false, 'future tool defaults', { pdf: pdf(), compatibility: status('unverified', 'unsupported'), primitive: 'action' }),
  toolChest: p('options.tool-chest', 'Options', 'Add to Tool Chest', 'Stores the configured markup as a reusable tool.', 'action', null, false, 'future tool preset', { pdf: pdf([], ['/BSIToolSet']), compatibility: status('unverified', 'unsupported'), primitive: 'action' }),
};

const GENERAL = ['subject', 'author', 'comments', 'locked'];
const MEASUREMENT_GENERAL = ['label'];
const STROKE = ['strokeColor', 'opacity', 'lineWidth', 'lineStyle'];
const FILL = ['fillColor', 'fillOpacity', 'highlight', 'hatchPattern', 'hatchColor', 'hatchScale'];
const LINE_FILL = ['fillColor', 'highlight'];
const ENDPOINTS = ['startEndpoint', 'endEndpoint', 'endpointScale', 'endpointFill'];
const TEXT = ['textColor', 'fontFamily', 'fontSize', 'autoSize', 'lineSpacing', 'margin', 'textAlign', 'fontStyles'];
const LAYOUT = ['x', 'y', 'width', 'height', 'rotation'];
const MEASURE = ['units', 'precision', 'scale', 'caption', 'captionPosition'];
const OPTIONS = ['setDefault', 'toolChest'];
const list = (...groups) => [...new Set(groups.flat())];

const toolSpecs = [
  ['text-box', 'Text Box', 'textBox', list(GENERAL, STROKE, FILL, TEXT, LAYOUT, OPTIONS)],
  ['rectangle', 'Rectangle', 'rectangle', list(GENERAL, STROKE, FILL, LAYOUT, OPTIONS)],
  ['ellipse', 'Ellipse', 'ellipse', list(GENERAL, STROKE, FILL, LAYOUT, OPTIONS)],
  ['arc', 'Arc', 'arc', list(GENERAL, STROKE, LINE_FILL, ENDPOINTS, LAYOUT, OPTIONS)],
  ['line', 'Line', 'line', list(GENERAL, STROKE, LINE_FILL, ENDPOINTS, LAYOUT, OPTIONS)],
  ['arrow', 'Arrow', 'line', list(GENERAL, STROKE, LINE_FILL, ENDPOINTS, LAYOUT, OPTIONS)],
  ['dimension', 'Dimension', 'dimension', list(GENERAL, MEASUREMENT_GENERAL, STROKE, LINE_FILL, ENDPOINTS, TEXT, MEASURE, LAYOUT, OPTIONS)],
  ['length', 'Length', 'length', list(GENERAL, MEASUREMENT_GENERAL, STROKE, LINE_FILL, ENDPOINTS, TEXT, MEASURE, ['slope'], LAYOUT, OPTIONS)],
  ['polylength', 'Polylength', 'polylength', list(GENERAL, MEASUREMENT_GENERAL, STROKE, LINE_FILL, ENDPOINTS, TEXT, MEASURE, ['slope'], LAYOUT, OPTIONS)],
  ['area', 'Area', 'area', list(GENERAL, MEASUREMENT_GENERAL, STROKE, FILL, TEXT, MEASURE, LAYOUT, OPTIONS)],
  ['polyline', 'Polyline', 'polyline', list(GENERAL, STROKE, LINE_FILL, ENDPOINTS, LAYOUT, OPTIONS)],
  ['polygon', 'Polygon', 'polygon', list(GENERAL, STROKE, FILL, LAYOUT, OPTIONS)],
  ['pen', 'Pen', 'pen', list(GENERAL, STROKE, LAYOUT, OPTIONS)],
  ['highlight', 'Highlight', 'highlight', list(GENERAL, ['strokeColor', 'opacity', 'lineWidth'], LAYOUT, OPTIONS)],
  ['cloud', 'Cloud', 'cloud', list(GENERAL, STROKE, FILL, ['cloudSize', 'invertCloud'], LAYOUT, OPTIONS)],
  ['cloud-plus', 'Cloud+', 'callout', list(GENERAL, STROKE, FILL, ENDPOINTS, TEXT, ['cloudSize', 'invertCloud', 'shape'], LAYOUT, OPTIONS)],
  ['callout', 'Callout', 'callout', list(GENERAL, STROKE, FILL, ENDPOINTS, TEXT, ['cloudSize', 'invertCloud', 'shape'], LAYOUT, OPTIONS)],
  ['image', 'Image', 'image', list(GENERAL, ['opacity'], LAYOUT, OPTIONS)],
  ['snapshot', 'Snapshot', 'snapshot', list(GENERAL, ['opacity'], LAYOUT, OPTIONS)],
  ['count', 'Count', 'count', list(GENERAL, MEASUREMENT_GENERAL, ['strokeColor', 'opacity'], TEXT, ['units', 'precision', 'caption', 'countSymbol', 'countScale', 'countWidth', 'countHeight', 'countDepth', 'countGroup'], OPTIONS)],
];

export const bluebeamPropertyContract = {
  schema: PROPERTY_CONTRACT_SCHEMA,
  version: PROPERTY_CONTRACT_VERSION,
  auditedAgainst: { documentation: 'Revu 21 user manual', installedApplication: evidenceCatalog.revu212Environment.version },
  scope: { included: '19 Butter Paper annotation tools plus Bluebeam Count', excluded: ['select', 'pan', 'advanced-measurement', 'sketch', 'stamp', 'form'] },
  statuses: PROPERTY_STATUSES,
  sources: sourceCatalog,
  evidence: evidenceCatalog,
  tools: toolSpecs.map(([id, label, sourceRef, propertyRefs]) => ({
    id, label, butterPaperImplemented: id !== 'count', sourceRefs: ['manual', 'propertiesPanel', 'propertiesToolbar', sourceRef],
    properties: propertyRefs.map((ref) => ({
      tool: id,
      ...definitions[ref],
      conditions: id === 'count' && TEXT.includes(ref)
        ? [{ property: 'measurement.caption', operator: 'is', value: true }, ...(definitions[ref].conditions ?? [])]
        : definitions[ref].conditions,
      compatibility: id === 'count'
        ? definitions[ref].compatibility
        : definitions[ref].section === 'Options'
          ? definitions[ref].compatibility
          : status('preserved-untouched', 'unverified'),
      sourceRefs: [...new Set([...(definitions[ref].sourceRefs ?? []), sourceRef])],
    })),
  })),
};

export function validatePropertyContract(contract = bluebeamPropertyContract) {
  const errors = [];
  if (contract?.schema !== PROPERTY_CONTRACT_SCHEMA) errors.push(`schema must be ${PROPERTY_CONTRACT_SCHEMA}`);
  if (contract?.version !== PROPERTY_CONTRACT_VERSION) errors.push(`version must be ${PROPERTY_CONTRACT_VERSION}`);
  if (contract?.tools?.length !== 20) errors.push(`expected exactly 20 audited tools, received ${contract?.tools?.length ?? 0}`);
  const toolIds = new Set();
  const globalKeys = new Set();
  for (const tool of contract?.tools ?? []) {
    if (!tool.id || toolIds.has(tool.id)) errors.push(`duplicate or missing tool id: ${tool.id}`);
    toolIds.add(tool.id);
    if (tool.id === 'count' && tool.butterPaperImplemented !== false) errors.push('Count must be marked missing from Butter Paper');
    const toolPropertyKeys = new Set((tool.properties ?? []).map((property) => property.key));
    for (const property of tool.properties ?? []) {
      const scopedKey = `${tool.id}:${property.key}`;
      if (globalKeys.has(scopedKey)) errors.push(`duplicate property key: ${scopedKey}`);
      globalKeys.add(scopedKey);
      for (const field of ['tool', 'key', 'section', 'label', 'meaning', 'valueType', 'affectedVisualComponent', 'primitive']) {
        if (!property[field]) errors.push(`${scopedKey}.${field} is required`);
      }
      if (property.tool !== tool.id) errors.push(`${scopedKey}.tool must match its parent tool`);
      if (!Object.hasOwn(property, 'default')) errors.push(`${scopedKey}.default is required`);
      if (!Array.isArray(property.conditions)) errors.push(`${scopedKey}.conditions must be an array`);
      for (const condition of property.conditions ?? []) {
        if (!condition.property || !condition.operator || !Object.hasOwn(condition, 'value')) errors.push(`${scopedKey} has an invalid condition`);
        for (const referencedKey of String(condition.property ?? '').split('|')) {
          if (referencedKey && !toolPropertyKeys.has(referencedKey)) errors.push(`${scopedKey} condition references missing property ${referencedKey}`);
        }
      }
      if (!property.sourceRefs?.length || property.sourceRefs.some((ref) => !contract.sources?.[ref])) errors.push(`${scopedKey} must have valid official source refs`);
      if (!property.evidenceRefs?.length || property.evidenceRefs.some((ref) => !contract.evidence?.[ref])) errors.push(`${scopedKey} must have complete evidence refs`);
      if (!property.pdf || !Array.isArray(property.pdf.standardKeys) || !Array.isArray(property.pdf.bluebeamKeys)
        || !Array.isArray(property.pdf.unverifiedBluebeamKeyCandidates)
        || !property.pdf.appearanceStream || !property.pdf.groupedAnnotations) errors.push(`${scopedKey} has incomplete PDF evidence fields`);
      for (const direction of ['revuToButter', 'butterToRevu']) {
        if (!PROPERTY_STATUSES.includes(property.compatibility?.[direction])) errors.push(`${scopedKey}.compatibility.${direction} is invalid`);
      }
    }
  }
  const expected = toolSpecs.map(([id]) => id);
  for (const id of expected) if (!toolIds.has(id)) errors.push(`missing audited tool: ${id}`);
  if (errors.length) {
    const error = new Error(`Invalid Bluebeam property contract:\n${errors.map((item) => `- ${item}`).join('\n')}`);
    error.code = 'BLUEBEAM_PROPERTY_CONTRACT_INVALID';
    error.errors = errors;
    throw error;
  }
  return contract;
}

export function renderPropertyContractMarkdown(contract = bluebeamPropertyContract) {
  validatePropertyContract(contract);
  const out = [
    '# Bluebeam Revu property audit', '',
    `Generated from \`scripts/bluebeam-compat/property-contract.mjs\` schema v${contract.version}.`, '',
    '> Evidence boundary: the installed Revu 21.2 manifest proves the application/capture environment. Properties without a one-variable PDF probe remain explicitly `unverified`; documentation is not treated as binary evidence.', '',
    '## Scope and compatibility vocabulary', '',
    `Audited: ${contract.tools.length} tools (${contract.tools.filter((tool) => tool.butterPaperImplemented).length} in Butter Paper plus Count). Excluded: ${contract.scope.excluded.join(', ')}.`, '',
    `Statuses: ${contract.statuses.map((value) => `\`${value}\``).join(', ')}.`, '',
    '## Cross-tool primitive index', '',
    '| Primitive | Used by |', '| --- | --- |',
  ];
  const primitives = new Map();
  for (const tool of contract.tools) for (const property of tool.properties) {
    const uses = primitives.get(property.primitive) ?? new Set(); uses.add(tool.label); primitives.set(property.primitive, uses);
  }
  for (const [primitive, uses] of [...primitives].sort(([a], [b]) => a.localeCompare(b))) out.push(`| ${escapeCell(primitive)} | ${escapeCell([...uses].join(', '))} |`);
  out.push('', '## Compatibility gaps', '', '| Tool | Property | Revu → Butter | Butter → Revu | Evidence note |', '| --- | --- | --- | --- | --- |');
  for (const tool of contract.tools) for (const property of tool.properties) {
    if (property.compatibility.revuToButter === 'exact' && property.compatibility.butterToRevu === 'exact') continue;
    out.push(`| ${escapeCell(tool.label)} | ${escapeCell(property.label)} | ${property.compatibility.revuToButter} | ${property.compatibility.butterToRevu} | ${property.evidenceRefs.includes('documentationOnly') ? 'Documentation only; probe required' : 'Probe attached'} |`);
  }
  for (const tool of contract.tools) {
    out.push('', `## ${tool.label}${tool.butterPaperImplemented ? '' : ' (not implemented in Butter Paper)'}`, '');
    out.push(`Sources: ${tool.sourceRefs.map((ref) => `[${contract.sources[ref].title}](${contract.sources[ref].url})`).join(', ')}.`, '');
    out.push('| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |', '| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |');
    for (const property of tool.properties) {
      const values = property.values === null ? property.valueType : typeof property.values === 'string' ? property.values : JSON.stringify(property.values);
      const condition = property.conditions.length ? property.conditions.map((item) => `${item.property} ${item.operator} ${JSON.stringify(item.value)}`).join('; ') : 'always';
      const keys = [...property.pdf.standardKeys, ...property.pdf.bluebeamKeys].join(', ') || 'none identified';
      const candidates = property.pdf.unverifiedBluebeamKeyCandidates.length ? `; unverified candidates: ${property.pdf.unverifiedBluebeamKeyCandidates.join(', ')}` : '';
      out.push(`| ${escapeCell(property.section)} | ${escapeCell(property.label)} | ${escapeCell(property.meaning)} | ${escapeCell(values)} | ${escapeCell(JSON.stringify(property.default))} | ${escapeCell(condition)} | ${escapeCell(property.affectedVisualComponent)} | ${escapeCell(`${keys}${candidates}; AS: ${property.pdf.appearanceStream}; grouped: ${property.pdf.groupedAnnotations}`)} | ${property.compatibility.revuToButter} | ${property.compatibility.butterToRevu} |`);
    }
  }
  out.push('', '## Required probe matrix', '',
    'For each property above, create one-variable-at-a-time specimens covering untouched save, Revu edit and resave, Butter edit and Revu reimport, and conditional transitions. Count additionally requires add/remove/regroup tests; Cloud+ requires both inline and externally associated components.', '',
    'Same-renderer comparisons isolate producer differences. Interoperability comparisons use fixed PDF hash, page geometry, scale/zoom, ROI, DPI, locale, fonts, background, and theme while allowing the viewer application and OS to differ. Both modes retain registration, SSIM, ink-mask IoU, boundary distance, luminance, continuity, and heatmap evidence.');
  return `${out.join('\n')}\n`;
}

export async function writePropertyContractReport(outputPath = resolve(dirname(fileURLToPath(import.meta.url)), '../../docs/compatibility/bluebeam-properties.md')) {
  await writeFile(outputPath, renderPropertyContractMarkdown(), 'utf8');
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const outputPath = process.argv[2] ? resolve(process.argv[2]) : undefined;
  writePropertyContractReport(outputPath).catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}

function escapeCell(value) {
  return String(value).replaceAll('|', '\\|').replaceAll('\n', ' ');
}
