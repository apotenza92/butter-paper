import type { Markup } from '@butter-paper/core';
import type { ToolMode } from '../../../shared/protocol';
import { ARC_TOOL_DEFINITION } from './builtins/arcTool';
import { CALLOUT_TOOL_DEFINITION } from './builtins/calloutTool';
import { CLOUD_PLUS_TOOL_DEFINITION } from './builtins/cloudPlusTool';
import { CLOUD_TOOL_DEFINITION } from './builtins/cloudTool';
import { DIMENSION_TOOL_DEFINITION } from './builtins/dimensionTool';
import { ELLIPSE_TOOL_DEFINITION } from './builtins/ellipseTool';
import { IMPORTED_ANNOTATION_TOOL_DEFINITION } from './builtins/importedAnnotationTool';
import { HIGHLIGHT_TOOL_DEFINITION, PEN_TOOL_DEFINITION } from './builtins/inkTool';
import { IMAGE_TOOL_DEFINITION } from './builtins/imageTool';
import { ARROW_TOOL_DEFINITION, LINE_TOOL_DEFINITION } from './builtins/lineTool';
import { AREA_TOOL_DEFINITION, LENGTH_TOOL_DEFINITION, POLYLENGTH_TOOL_DEFINITION } from './builtins/measurementTool';
import { POLYLINE_TOOL_DEFINITION } from './builtins/polylineTool';
import { POLYGON_TOOL_DEFINITION } from './builtins/polygonTool';
import { RECTANGLE_TOOL_DEFINITION } from './builtins/rectangleTool';
import { SNAPSHOT_TOOL_DEFINITION } from './builtins/snapshotTool';
import { TEXT_BOX_TOOL_DEFINITION } from './builtins/textBoxTool';
import type { PdfToolDefinition } from './types';

type RegistryToolDefinition = PdfToolDefinition & { readonly id: ToolMode };

export const PDF_TOOL_REGISTRY = [
  {
    id: 'select',
    label: 'Select',
    shortcut: 'V',
    category: 'navigation',
    cursor: 'default',
    testId: 'tool-select',
    implemented: true,
    properties: { properties: [] },
    defaults: {},
  },
  {
    id: 'pan',
    label: 'Pan',
    shortcut: 'Space',
    category: 'navigation',
    cursor: 'grab',
    testId: 'tool-pan',
    implemented: true,
    properties: { properties: [] },
    defaults: {},
  },
  TEXT_BOX_TOOL_DEFINITION,
  RECTANGLE_TOOL_DEFINITION,
  ELLIPSE_TOOL_DEFINITION,
  ARC_TOOL_DEFINITION,
  LINE_TOOL_DEFINITION,
  ARROW_TOOL_DEFINITION,
  DIMENSION_TOOL_DEFINITION,
  LENGTH_TOOL_DEFINITION,
  POLYLENGTH_TOOL_DEFINITION,
  AREA_TOOL_DEFINITION,
  POLYLINE_TOOL_DEFINITION,
  POLYGON_TOOL_DEFINITION,
  PEN_TOOL_DEFINITION,
  HIGHLIGHT_TOOL_DEFINITION,
  CLOUD_TOOL_DEFINITION,
  CLOUD_PLUS_TOOL_DEFINITION,
  CALLOUT_TOOL_DEFINITION,
  IMAGE_TOOL_DEFINITION,
  SNAPSHOT_TOOL_DEFINITION,
] as const satisfies readonly RegistryToolDefinition[];

export function getToolDefinition(tool: ToolMode): RegistryToolDefinition {
  const definition = PDF_TOOL_REGISTRY.find((candidate) => candidate.id === tool);
  if (!definition) {
    throw new Error(`Unknown PDF tool: ${tool}`);
  }

  return definition;
}

export function getMarkupToolDefinition(markup: Markup): PdfToolDefinition | null {
  if (markup.kind === 'rectangle') {
    return RECTANGLE_TOOL_DEFINITION;
  }

  if (markup.kind === 'ellipse') {
    return ELLIPSE_TOOL_DEFINITION;
  }

  if (markup.kind === 'arc') {
    return ARC_TOOL_DEFINITION;
  }

  if (markup.kind === 'line') {
    return LINE_TOOL_DEFINITION;
  }

  if (markup.kind === 'arrow') {
    return ARROW_TOOL_DEFINITION;
  }

  if (markup.kind === 'dimension') {
    return DIMENSION_TOOL_DEFINITION;
  }

  if (markup.kind === 'length') {
    return LENGTH_TOOL_DEFINITION;
  }

  if (markup.kind === 'polylength') {
    return POLYLENGTH_TOOL_DEFINITION;
  }

  if (markup.kind === 'area') {
    return AREA_TOOL_DEFINITION;
  }

  if (markup.kind === 'polyline') {
    return POLYLINE_TOOL_DEFINITION;
  }

  if (markup.kind === 'polygon') {
    return POLYGON_TOOL_DEFINITION;
  }

  if (markup.kind === 'pen') {
    return PEN_TOOL_DEFINITION;
  }

  if (markup.kind === 'highlight') {
    return HIGHLIGHT_TOOL_DEFINITION;
  }

  if (markup.kind === 'cloud') {
    return CLOUD_TOOL_DEFINITION;
  }

  if (markup.kind === 'cloud-plus') {
    return CLOUD_PLUS_TOOL_DEFINITION;
  }

  if (markup.kind === 'callout') {
    return CALLOUT_TOOL_DEFINITION;
  }

  if (markup.kind === 'image') {
    return IMAGE_TOOL_DEFINITION;
  }

  if (markup.kind === 'snapshot') {
    return SNAPSHOT_TOOL_DEFINITION;
  }

  if (markup.kind === 'text-box') {
    return TEXT_BOX_TOOL_DEFINITION;
  }

  if (markup.kind === 'imported-annotation') {
    return IMPORTED_ANNOTATION_TOOL_DEFINITION;
  }

  return null;
}
