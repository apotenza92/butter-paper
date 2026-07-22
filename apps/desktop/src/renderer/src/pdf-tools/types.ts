import type { DocumentModel, Markup, PageModel, PageScale, PageTransform, PdfPoint, Rect, TextBoxRichTextRun } from '@butter-paper/core';
import type { ToolMode } from '../../../shared/protocol';

export type ToolId = ToolMode;
export type MarkupToolId = ToolMode | 'imported-annotation';
export type ToolCategory = 'navigation' | 'markup' | 'measurement' | 'media';
export type InteractionPhase = 'idle' | 'hovered' | 'selected' | 'focused' | 'draft' | 'dragging';
export type BodyDragBehavior = 'moveGroup' | 'moveSelf' | 'adjustOnly';
export type HandleBehavior = 'resizeSelf' | 'rotateSelf' | 'moveEndpoint' | 'moveKnee' | 'reshapeVertex' | 'reshapeArc';
export type ComponentRole = 'bounds' | 'shape' | 'textBox' | 'leader' | 'endpoint' | 'measurement' | 'media';
export type SelectionBoundsKind = 'child' | 'group';

export interface ToolContext {
  readonly page: PageModel;
  readonly pageScale?: PageScale;
}

export interface ToolRenderContext extends ToolContext {
  readonly phase: InteractionPhase;
}

export interface ToolHitContext extends ToolContext {
  readonly tolerance: number;
  readonly transform?: PageTransform;
}

export interface ToolSelectionContext extends ToolContext {
  readonly phase: Extract<InteractionPhase, 'hovered' | 'selected' | 'focused' | 'draft'>;
}

export type GeometryPrimitive =
  | { readonly kind: 'rect'; readonly rect: Rect; readonly rotation?: number }
  | { readonly kind: 'line'; readonly start: PdfPoint; readonly end: PdfPoint }
  | { readonly kind: 'polyline'; readonly points: readonly PdfPoint[] }
  | { readonly kind: 'vertexPath'; readonly points: readonly PdfPoint[]; readonly closed: boolean }
  | { readonly kind: 'textBox'; readonly rect: Rect; readonly rotation?: number }
  | { readonly kind: 'generatedPath'; readonly controlPath: readonly PdfPoint[]; readonly closed: boolean; readonly lineType: LineTypeReference };

export interface LineTypeReference<TOptions = Record<string, unknown>> {
  readonly id: string;
  readonly options: TOptions;
  readonly pdfCompatibility?: Record<string, unknown>;
}

export interface ToolComponentDescriptor {
  readonly id: string;
  readonly role: ComponentRole;
  readonly geometry: GeometryPrimitive;
  readonly bodyDrag: BodyDragBehavior;
}

export interface ToolHandleDescriptor {
  readonly id: string;
  readonly componentId: string;
  readonly point: PdfPoint;
  readonly behavior: HandleBehavior;
  readonly cursor: string;
}

export interface ToolGeometryDescriptor {
  readonly bounds: Rect;
  readonly components: readonly ToolComponentDescriptor[];
  readonly handles?: readonly ToolHandleDescriptor[];
}

export type ContentStyle = {
  readonly stroke?: string;
  readonly fill?: string;
  readonly strokeWidth?: number;
  readonly opacity?: number;
  readonly dashArray?: string;
  readonly hideBelowZoom?: number;
  readonly lineCap?: 'butt' | 'round' | 'square';
  readonly lineJoin?: 'miter' | 'round' | 'bevel';
  readonly blendMode?: 'multiply';
};

export type TextContentStyle = ContentStyle & {
  readonly fontFamily?: string;
  readonly fontSizePt?: number;
  readonly textColor?: string;
  readonly textAlign?: 'left' | 'center' | 'right';
  readonly lineHeightPt?: number;
  readonly textInsetPt?: number;
  readonly firstBaselineOffsetPt?: number;
};

export type RenderPrimitive =
  | { readonly kind: 'rect'; readonly rect: Rect; readonly rotation?: number; readonly style: ContentStyle; readonly pointerEvents?: 'none' | 'visibleStroke' | 'all' }
  | { readonly kind: 'ellipse'; readonly rect: Rect; readonly rotation?: number; readonly style: ContentStyle; readonly pointerEvents?: 'none' | 'visibleStroke' | 'all' }
  | { readonly kind: 'polygon'; readonly points: readonly PdfPoint[]; readonly style: ContentStyle; readonly pointerEvents?: 'none' | 'visibleStroke' | 'all' }
  | { readonly kind: 'polyline'; readonly points: readonly PdfPoint[]; readonly style: ContentStyle; readonly pointerEvents?: 'none' | 'visibleStroke' | 'all' }
  | { readonly kind: 'path'; readonly d: string; readonly style: ContentStyle; readonly pointerEvents?: 'none' | 'visibleStroke' | 'all' }
  | { readonly kind: 'textBox'; readonly rect: Rect; readonly text: string; readonly textLines?: readonly string[]; readonly richTextRuns?: readonly TextBoxRichTextRun[]; readonly rotation?: number; readonly style: TextContentStyle; readonly pointerEvents?: 'none' | 'all' }
  | { readonly kind: 'image'; readonly rect: Rect; readonly assetId: string; readonly rotation?: number; readonly opacity?: number; readonly pointerEvents?: 'none' | 'all' };

export interface SelectionBoundsDescriptor {
  readonly rect: Rect;
  readonly rotation?: number;
  readonly kind: SelectionBoundsKind;
  readonly canResize: boolean;
  readonly canRotate: boolean;
}

export interface SelectionChromeDescriptor {
  readonly bounds?: SelectionBoundsDescriptor;
  readonly handles?: readonly ToolHandleDescriptor[];
  readonly controlPaths?: readonly {
    readonly id: string;
    readonly points: readonly PdfPoint[];
    readonly closed: boolean;
  }[];
}

export interface ToolHit {
  readonly markupId: string;
  readonly componentId: string;
  readonly region: 'edge' | 'interior' | 'handle' | 'vertex' | 'leader';
  readonly handleId?: string;
  readonly bodyDrag?: BodyDragBehavior;
  readonly handleBehavior?: HandleBehavior;
  readonly cursor?: string;
}

export interface ToolGeometryProvider<TMarkup extends Markup = Markup> {
  getGeometry(markup: TMarkup, context: ToolContext): ToolGeometryDescriptor;
  hitTest(markup: TMarkup, point: PdfPoint, context: ToolHitContext): ToolHit | null;
}

export interface ToolRenderProvider<TMarkup extends Markup = Markup, TDraft = unknown> {
  getContentPrimitives(markup: TMarkup, context: ToolRenderContext): readonly RenderPrimitive[];
  getDraftPrimitives?(draft: TDraft, context: ToolRenderContext): readonly RenderPrimitive[];
}

export interface ToolSelectionProvider<TMarkup extends Markup = Markup, TDraft = unknown> {
  getSelectionChrome(markup: TMarkup, context: ToolSelectionContext): SelectionChromeDescriptor;
  getDraftChrome?(draft: TDraft, context: ToolSelectionContext): SelectionChromeDescriptor;
}

export interface ToolInteractionSession {
  readonly pointerId: number;
  readonly startPoint: PdfPoint;
  readonly currentPoint: PdfPoint;
}

export interface ToolInteractionResult<TDraft = unknown> {
  readonly draft?: TDraft | null;
  readonly selectMarkupIds?: readonly string[];
  readonly updateDocument?: (document: DocumentModel) => DocumentModel;
  readonly commitMarkup?: Markup;
  readonly capturePointer?: boolean;
}

export interface ToolInteractionProvider<TMarkup extends Markup = Markup, TDraft = unknown> {
  readonly placement?: 'drag' | 'click';
  createDraft?(session: ToolInteractionSession): TDraft;
  updateDraft?(draft: TDraft, point: PdfPoint): TDraft;
  commitDraft?(draft: TDraft, context: ToolContext & {
    readonly hasExceededDragThreshold: boolean;
    createMarkupId(prefix: string): string;
  }): TMarkup | null;
  transformMarkup?(markup: TMarkup, input: {
    readonly handleId: string;
    readonly handleBehavior: HandleBehavior;
    readonly startPoint: PdfPoint;
    readonly currentPoint: PdfPoint;
  }): TMarkup;
  dragMarkup?(markup: TMarkup, input: {
    readonly componentId: string;
    readonly bodyDrag: BodyDragBehavior;
    readonly delta: PdfPoint;
  }): TMarkup;
}

export type ToolPropertyDefinition =
  | { readonly kind: 'color'; readonly key: string; readonly label: string; readonly default: string | null }
  | { readonly kind: 'number'; readonly key: string; readonly label: string; readonly default: number; readonly min?: number; readonly max?: number; readonly step?: number }
  | { readonly kind: 'select'; readonly key: string; readonly label: string; readonly default: string; readonly options: readonly { readonly value: string; readonly label: string }[] }
  | { readonly kind: 'boolean'; readonly key: string; readonly label: string; readonly default: boolean };

export interface ToolPropertySchema {
  readonly properties: readonly ToolPropertyDefinition[];
}

export interface PdfAnnotationLike {
  readonly subtype?: string;
  readonly intent?: string;
  readonly subject?: string;
  readonly rect?: readonly number[];
  readonly fields?: Record<string, unknown>;
}

export interface PdfMappingHooks<TMarkup extends Markup = Markup> {
  canImport(annotation: PdfAnnotationLike): boolean;
  import(annotation: PdfAnnotationLike, context: { readonly pageIndex: number; readonly fallbackId: string }): TMarkup;
}

export interface PdfToolDefinition<TMarkup extends Markup = Markup, TDraft = unknown> {
  readonly id: MarkupToolId;
  readonly label: string;
  readonly shortcut?: string;
  readonly category: ToolCategory;
  readonly cursor: string;
  readonly testId: string;
  readonly implemented: true;
  readonly properties: ToolPropertySchema;
  readonly defaults: Record<string, unknown>;
  readonly geometry?: ToolGeometryProvider<TMarkup>;
  readonly render?: ToolRenderProvider<TMarkup, TDraft>;
  readonly selection?: ToolSelectionProvider<TMarkup, TDraft>;
  readonly interaction?: ToolInteractionProvider<TMarkup, TDraft>;
  readonly pdf?: PdfMappingHooks<TMarkup>;
}
