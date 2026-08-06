import type { PdfPoint, Rect, Size } from './points.js';

export type MarkupId = string;

export interface PageModel {
  readonly id: string;
  readonly index: number;
  /** Effective visible PDF page box in unrotated default-user-space coordinates. */
  readonly viewBox?: Rect;
  /** PDF /UserUnit multiplier. Real loaded pages provide this; legacy models default to 1. */
  readonly userUnit?: number;
  readonly size: Size;
  readonly rotation: 0 | 90 | 180 | 270;
}

export interface DocumentMetadata {
  readonly title?: string;
  readonly author?: string;
  readonly subject?: string;
  readonly creator?: string;
  readonly producer?: string;
}

export interface DocumentModel {
  readonly id: string;
  readonly path: string;
  readonly metadata: DocumentMetadata;
  readonly pages: readonly PageModel[];
  readonly markups: readonly Markup[];
  readonly pageScales?: readonly PageScale[];
  readonly scalePresets?: readonly ScalePreset[];
}

export type PageRotationDirection = 'left' | 'right';

export type ScaleSource = 'preset' | 'custom' | 'calibrated';
export type ScaleUnit = 'in' | 'ft' | 'mm' | 'cm' | 'm';
export type ScalePrecisionMode = 'decimal' | 'fraction';

export interface ScalePrecision {
  readonly mode: ScalePrecisionMode;
  readonly value: number;
}

export interface PageScale {
  readonly pageIndex: number;
  readonly source: ScaleSource;
  readonly name: string;
  readonly pdfUnits: ScaleUnit;
  readonly realUnits: ScaleUnit;
  readonly scaleX: number;
  readonly scaleY: number;
  readonly precision: ScalePrecision;
}

export interface ScalePreset {
  readonly id: string;
  readonly name: string;
  readonly pdfUnits: ScaleUnit;
  readonly realUnits: ScaleUnit;
  readonly scaleX: number;
  readonly scaleY: number;
  readonly source: Extract<ScaleSource, 'preset' | 'custom' | 'calibrated'>;
  readonly builtIn: boolean;
}

export interface MarkupSource {
  readonly annotationId?: string;
  /** All native PDF annotation objects that make up this logical markup. */
  readonly annotationIds?: readonly string[];
  /**
   * Audited, non-geometric metadata from each native annotation object.
   * Geometry, appearances, object ids and relationships are always rebuilt by
   * the target document writer instead of being copied through this model.
   */
  readonly annotationMetadata?: readonly AnnotationMetadata[];
  /** Stable snapshot used to preserve untouched imported annotations verbatim. */
  readonly originalFingerprint?: string;
  readonly source?: 'butter' | 'imported';
}

export type AnnotationMetadataRole = 'primary' | 'cloud' | 'text';

export interface AnnotationMetadata {
  readonly annotationId: string;
  readonly role?: AnnotationMetadataRole;
  readonly author?: string;
  readonly subject?: string;
  readonly creationDate?: string;
  readonly modificationDate?: string;
  readonly contents?: string;
  readonly flags?: number;
  readonly status?: string;
  readonly statusModel?: string;
  readonly replyType?: 'Reply' | 'Group';
  readonly replyToAnnotationId?: string;
}

export interface AnnotationStrokeAppearance {
  readonly color: string;
  readonly widthPt: number;
}

export interface AnnotationFillAppearance {
  readonly color: string | null;
}

export interface AnnotationTextAppearance {
  readonly color: string;
  readonly fontId: string;
  readonly fontSizePt: number;
  readonly lineHeightPt: number;
  readonly align: 'left' | 'center' | 'right';
  readonly insetPt: number;
}

export interface MarkupAppearance {
  readonly stroke?: Partial<AnnotationStrokeAppearance>;
  readonly fill?: Partial<AnnotationFillAppearance>;
  readonly text?: Partial<AnnotationTextAppearance>;
  readonly opacity?: number;
  readonly blendMode?: 'normal' | 'multiply';
}

export interface ResolvedMarkupAppearance {
  readonly stroke?: AnnotationStrokeAppearance;
  readonly fill?: AnnotationFillAppearance;
  readonly text?: AnnotationTextAppearance;
  readonly opacity: number;
  readonly blendMode: 'normal' | 'multiply';
}

export interface MarkupBase {
  readonly id: MarkupId;
  readonly pageIndex: number;
  readonly color?: string;
  readonly opacity?: number;
  readonly appearance?: MarkupAppearance;
  readonly source?: MarkupSource;
}

export interface RectangleMarkup extends MarkupBase {
  readonly kind: 'rectangle';
  readonly rect: Rect;
  readonly rotation?: number;
}

export interface EllipseMarkup extends MarkupBase {
  readonly kind: 'ellipse';
  readonly rect: Rect;
  readonly rotation?: number;
}

export interface ArcMarkup extends MarkupBase {
  readonly kind: 'arc';
  readonly rect: Rect;
  readonly angle1: number;
  readonly angle2: number;
  readonly start?: PdfPoint;
  readonly end?: PdfPoint;
  readonly mid?: PdfPoint;
}

export interface LineMarkup extends MarkupBase {
  readonly kind: 'line';
  readonly start: PdfPoint;
  readonly end: PdfPoint;
}

export interface ArrowMarkup extends MarkupBase {
  readonly kind: 'arrow';
  readonly start: PdfPoint;
  readonly end: PdfPoint;
}

export interface DimensionMarkup extends MarkupBase {
  readonly kind: 'dimension';
  readonly start: PdfPoint;
  readonly end: PdfPoint;
  readonly dimensionLineOffset: number;
  readonly text: string;
}

export interface LengthMarkup extends MarkupBase {
  readonly kind: 'length';
  readonly start: PdfPoint;
  readonly end: PdfPoint;
  readonly displayUnit?: ScaleUnit;
}

export interface PolylengthMarkup extends MarkupBase {
  readonly kind: 'polylength';
  readonly points: readonly PdfPoint[];
  readonly displayUnit?: ScaleUnit;
}

export interface AreaMarkup extends MarkupBase {
  readonly kind: 'area';
  readonly points: readonly PdfPoint[];
  readonly displayUnit?: ScaleUnit;
}

export interface PolylineMarkup extends MarkupBase {
  readonly kind: 'polyline';
  readonly points: readonly PdfPoint[];
}

export interface PolygonMarkup extends MarkupBase {
  readonly kind: 'polygon';
  readonly points: readonly PdfPoint[];
}

export interface PenMarkup extends MarkupBase {
  readonly kind: 'pen';
  readonly paths: readonly (readonly PdfPoint[])[];
  readonly strokeWidth?: number;
}

export interface HighlightMarkup extends MarkupBase {
  readonly kind: 'highlight';
  readonly paths: readonly (readonly PdfPoint[])[];
  readonly strokeWidth?: number;
  readonly blendMode?: 'multiply';
}

export interface CloudMarkup extends MarkupBase {
  readonly kind: 'cloud';
  readonly controlPath: readonly PdfPoint[];
  readonly strokeWidth?: number;
  readonly borderEffectIntensity?: number;
  readonly scallopRadius?: number;
  readonly appearancePath?: string;
}

export interface CloudPlusMarkup extends MarkupBase {
  readonly kind: 'cloud-plus';
  readonly cloud: {
    readonly controlPath: readonly PdfPoint[];
    readonly strokeWidth?: number;
    readonly borderEffectIntensity?: number;
    readonly scallopRadius?: number;
    readonly appearancePath?: string;
  };
  readonly leader: CalloutLeader;
  readonly textBox: Rect;
  readonly text: string;
}

export interface TextBoxRichTextRun {
  readonly text: string;
  readonly bold?: boolean;
  readonly italic?: boolean;
  readonly color?: string;
  readonly fontSizePt?: number;
}

export interface TextBoxMarkup extends MarkupBase {
  readonly kind: 'text-box';
  readonly rect: Rect;
  readonly text: string;
  readonly richTextRuns?: readonly TextBoxRichTextRun[];
  readonly rotation?: number;
  readonly appearanceTextLines?: readonly string[];
  readonly borderColor?: string;
  readonly borderWidth?: number;
  readonly fontFamily?: 'Helvetica' | 'ArialUnicode';
  readonly fontSizePt?: number;
  readonly lineHeightPt?: number;
  readonly textAlign?: 'left' | 'center' | 'right';
}

export interface CalloutLeader {
  readonly points: readonly PdfPoint[];
}

export interface CalloutMarkup extends MarkupBase {
  readonly kind: 'callout';
  readonly leader: CalloutLeader;
  readonly textBox: Rect;
  readonly text: string;
}

export interface ImageMarkup extends MarkupBase {
  readonly kind: 'image';
  readonly rect: Rect;
  readonly dataUrl: string;
  readonly mimeType: 'image/png' | 'image/jpeg';
  readonly rotation?: number;
}

export interface SnapshotMarkup extends MarkupBase {
  readonly kind: 'snapshot';
  readonly rect: Rect;
  readonly dataUrl: string;
  readonly mimeType: 'image/png' | 'image/jpeg';
  readonly rotation?: number;
}

export interface ImportedAnnotationMarkup extends MarkupBase {
  readonly kind: 'imported-annotation';
  readonly rect: Rect;
  readonly subtype: string;
  readonly subject?: string;
  readonly intent?: string;
  readonly contents?: string;
}

export type Markup = RectangleMarkup | EllipseMarkup | ArcMarkup | LineMarkup | ArrowMarkup | DimensionMarkup | LengthMarkup | PolylengthMarkup | AreaMarkup | PolylineMarkup | PolygonMarkup | PenMarkup | HighlightMarkup | CloudMarkup | CloudPlusMarkup | TextBoxMarkup | CalloutMarkup | ImageMarkup | SnapshotMarkup | ImportedAnnotationMarkup;

export interface SelectionState {
  readonly markupIds: readonly MarkupId[];
  readonly focusedMarkupId?: MarkupId;
}

export type ToolState =
  | { readonly kind: 'select' }
  | { readonly kind: 'pan' }
  | { readonly kind: 'rectangle' }
  | { readonly kind: 'ellipse' }
  | { readonly kind: 'arc' }
  | { readonly kind: 'line' }
  | { readonly kind: 'arrow' }
  | { readonly kind: 'dimension' }
  | { readonly kind: 'length' }
  | { readonly kind: 'polylength' }
  | { readonly kind: 'area' }
  | { readonly kind: 'polyline' }
  | { readonly kind: 'polygon' }
  | { readonly kind: 'pen' }
  | { readonly kind: 'highlight' }
  | { readonly kind: 'cloud' }
  | { readonly kind: 'cloud-plus' }
  | { readonly kind: 'text-box' }
  | { readonly kind: 'callout' }
  | { readonly kind: 'image' }
  | { readonly kind: 'snapshot' };

export interface ViewportState {
  readonly zoom: number;
  readonly pan: PdfPoint;
  readonly pageSpacing: number;
}

export interface WorkspaceState {
  readonly document: DocumentModel;
  readonly selection: SelectionState;
  readonly tool: ToolState;
  readonly viewport: ViewportState;
}

export function createDocument(
  params: Omit<DocumentModel, 'markups'> & { readonly markups?: readonly Markup[] },
): DocumentModel {
  return {
    ...params,
    markups: params.markups ?? [],
    pageScales: params.pageScales ?? [],
    scalePresets: params.scalePresets ?? [],
  };
}

export function rotateDocumentPage(
  document: DocumentModel,
  pageIndex: number,
  direction: PageRotationDirection,
): DocumentModel {
  const delta = direction === 'right' ? 90 : -90;
  return {
    ...document,
    pages: document.pages.map((page) => {
      if (page.index !== pageIndex) {
        return page;
      }

      return {
        ...page,
        size: {
          width: page.size.height,
          height: page.size.width,
        },
        rotation: ((page.rotation + delta + 360) % 360) as PageModel['rotation'],
      };
    }),
  };
}

export function createSelectionState(
  markupIds: readonly MarkupId[] = [],
  focusedMarkupId?: MarkupId,
): SelectionState {
  return {
    markupIds: uniqueIds(markupIds),
    focusedMarkupId: focusedMarkupId ?? markupIds[0],
  };
}

export function createViewportState(
  zoom = 1,
  pan: PdfPoint = { x: 0, y: 0 } as PdfPoint,
  pageSpacing = 24,
): ViewportState {
  return {
    zoom,
    pan,
    pageSpacing,
  };
}

export function uniqueIds(ids: readonly MarkupId[]): readonly MarkupId[] {
  return [...new Set(ids)];
}
