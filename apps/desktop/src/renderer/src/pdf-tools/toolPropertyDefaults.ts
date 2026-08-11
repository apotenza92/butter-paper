import { resolveMarkupAppearance, type Markup, type MarkupAppearance } from '@butter-paper/core';
import type { ToolMode } from '../../../shared/protocol';
import { PDF_TOOL_REGISTRY, getToolDefinition } from './toolRegistry';

export type ToolPropertyValue = string | number | boolean | null;
export type ToolPropertyValues = Readonly<Record<string, ToolPropertyValue>>;
export type ToolPropertyValuesByTool = Readonly<Partial<Record<ToolMode, ToolPropertyValues>>>;

export function createInitialToolPropertyValues(): ToolPropertyValuesByTool {
  return Object.fromEntries(PDF_TOOL_REGISTRY.map((tool) => [tool.id, builtInToolPropertyValues(tool.id)]));
}

export function builtInToolPropertyValues(tool: ToolMode): ToolPropertyValues {
  const definition = getToolDefinition(tool);
  return Object.fromEntries(definition.properties.properties.map((property) => [
    property.key,
    normalizeToolPropertyValue(definition.defaults[property.key] ?? property.default),
  ]));
}

export function applyToolPropertyValues(markup: Markup, values: ToolPropertyValues | undefined): Markup {
  if (!values) return markup;

  const resolved = resolveMarkupAppearance(markup);
  const appearance: MarkupAppearance = {
    ...markup.appearance,
    ...(resolved.stroke ? {
      stroke: {
        ...markup.appearance?.stroke,
        ...(typeof values.strokeColor === 'string' ? { color: values.strokeColor } : {}),
        ...(typeof values.strokeWidthPt === 'number' ? { widthPt: values.strokeWidthPt } : {}),
      },
    } : {}),
    ...(resolved.fill ? {
      fill: {
        ...markup.appearance?.fill,
        ...(values.fillColor === null || typeof values.fillColor === 'string' ? { color: values.fillColor } : {}),
      },
    } : {}),
    ...(resolved.text ? {
      text: {
        ...markup.appearance?.text,
        ...(typeof values.textColor === 'string' ? { color: values.textColor } : {}),
        ...(typeof values.fontFamily === 'string' ? { fontId: values.fontFamily } : {}),
        ...(typeof values.fontSizePt === 'number' ? {
          fontSizePt: values.fontSizePt,
          lineHeightPt: values.fontSizePt * (resolved.text.lineHeightPt / resolved.text.fontSizePt),
        } : {}),
      },
    } : {}),
    ...(typeof values.opacity === 'number' ? { opacity: values.opacity } : {}),
  };

  const next = { ...markup, appearance } as Markup;
  if (next.kind === 'cloud' && typeof values.cloudIntensity === 'number') {
    return { ...next, borderEffectIntensity: values.cloudIntensity };
  }
  if (next.kind === 'cloud-plus' && typeof values.cloudIntensity === 'number') {
    return { ...next, cloud: { ...next.cloud, borderEffectIntensity: values.cloudIntensity } };
  }
  if (next.kind === 'pen' && typeof values.smoothCurves === 'boolean') {
    return { ...next, smoothCurves: values.smoothCurves };
  }
  return next;
}

function normalizeToolPropertyValue(value: unknown): ToolPropertyValue {
  return typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean' || value === null
    ? value
    : null;
}
