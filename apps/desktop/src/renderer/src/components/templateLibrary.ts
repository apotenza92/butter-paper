import type { BlankPdfCreateRequest, ImportedPdfTemplateRecord } from '../../../shared/protocol';
import {
  DEFAULT_BLANK_PDF_SETTINGS,
  resolveBlankPdfDimensions,
  type BlankPdfPatternType,
  type BlankPdfSettings,
} from './blankPdfSettings';

export interface GeneratedPdfTemplate {
  readonly id: string;
  readonly name: string;
  readonly kind: 'generated';
  readonly builtIn: boolean;
  readonly settings: BlankPdfSettings;
}

export type ImportedPdfTemplate = ImportedPdfTemplateRecord & { readonly builtIn: false };
export type PdfTemplate = GeneratedPdfTemplate | ImportedPdfTemplate;

interface TemplateLibrarySnapshot {
  readonly version: 1;
  readonly customTemplates: readonly GeneratedPdfTemplate[];
  readonly importedTemplates: readonly ImportedPdfTemplate[];
  readonly lastTemplateId: string;
}

interface TemplateStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export const TEMPLATE_LIBRARY_STORAGE_KEY = 'butter-paper.template-library.v1';

const BUILT_IN_PATTERN_NAMES: ReadonlyArray<readonly [BlankPdfPatternType, string]> = [
  ['blank', 'Blank Paper'],
  ['dots', 'Dot Grid'],
  ['grid', 'Square Grid'],
  ['lined', 'Ruled Paper'],
  ['isometric', 'Isometric Grid'],
  ['triangle', 'Triangle Grid'],
];

export const BUILT_IN_TEMPLATES: readonly GeneratedPdfTemplate[] = BUILT_IN_PATTERN_NAMES.map(([patternType, name]) => ({
  id: `built-in-${patternType}`,
  name,
  kind: 'generated',
  builtIn: true,
  settings: { ...DEFAULT_BLANK_PDF_SETTINGS, patternType },
}));

export function loadTemplateLibrary(storage: TemplateStorage): TemplateLibrarySnapshot {
  const fallback = migrateBlankPdfDefault(storage);
  const raw = storage.getItem(TEMPLATE_LIBRARY_STORAGE_KEY);
  if (!raw) return fallback;
  try {
    const candidate = JSON.parse(raw) as Partial<TemplateLibrarySnapshot>;
    const customTemplates = Array.isArray(candidate.customTemplates)
      ? candidate.customTemplates.filter(isValidCustomTemplate)
      : [];
    const templates = [...BUILT_IN_TEMPLATES, ...customTemplates];
    // Imported templates arrive asynchronously from the main process. Preserve
    // their stable ID until withImportedTemplates can validate it.
    const candidateLastTemplateId =
      typeof candidate.lastTemplateId === 'string' ? candidate.lastTemplateId : undefined;
    const lastTemplateId =
      candidateLastTemplateId &&
      (templates.some((template) => template.id === candidateLastTemplateId) ||
        candidateLastTemplateId.startsWith('imported-'))
        ? candidateLastTemplateId
        : fallback.lastTemplateId;
    return { version: 1, customTemplates, importedTemplates: [], lastTemplateId };
  } catch {
    return fallback;
  }
}

export function saveTemplateLibrary(storage: TemplateStorage, snapshot: TemplateLibrarySnapshot): void {
  storage.setItem(TEMPLATE_LIBRARY_STORAGE_KEY, JSON.stringify({
    version: 1,
    customTemplates: snapshot.customTemplates,
    lastTemplateId: snapshot.lastTemplateId,
  }));
}

export function allTemplates(snapshot: TemplateLibrarySnapshot): readonly PdfTemplate[] {
  return [...BUILT_IN_TEMPLATES, ...snapshot.customTemplates, ...snapshot.importedTemplates];
}

export function withImportedTemplates(snapshot: TemplateLibrarySnapshot, records: readonly ImportedPdfTemplateRecord[]): TemplateLibrarySnapshot {
  const importedTemplates = records.map((record) => ({ ...record, builtIn: false as const }));
  const available = [...BUILT_IN_TEMPLATES, ...snapshot.customTemplates, ...importedTemplates];
  return {
    ...snapshot,
    importedTemplates,
    lastTemplateId: available.some((template) => template.id === snapshot.lastTemplateId)
      ? snapshot.lastTemplateId
      : BUILT_IN_TEMPLATES[0].id,
  };
}

export function lastTemplate(snapshot: TemplateLibrarySnapshot): PdfTemplate {
  return allTemplates(snapshot).find((template) => template.id === snapshot.lastTemplateId)
    ?? BUILT_IN_TEMPLATES[0];
}

export function useTemplate(snapshot: TemplateLibrarySnapshot, templateId: string): TemplateLibrarySnapshot {
  return allTemplates(snapshot).some((template) => template.id === templateId)
    ? { ...snapshot, lastTemplateId: templateId }
    : snapshot;
}

export function addGeneratedTemplate(
  snapshot: TemplateLibrarySnapshot,
  name: string,
  settings: BlankPdfSettings,
  id: string = crypto.randomUUID(),
): TemplateLibrarySnapshot {
  const template: PdfTemplate = {
    id: `custom-${id}`,
    name: normalizedTemplateName(name),
    kind: 'generated',
    builtIn: false,
    settings: validatedSettings(settings),
  };
  return { ...snapshot, customTemplates: [...snapshot.customTemplates, template], lastTemplateId: template.id };
}

export function removeTemplate(snapshot: TemplateLibrarySnapshot, templateId: string): TemplateLibrarySnapshot {
  const customTemplates = snapshot.customTemplates.filter((template) => template.id !== templateId);
  const importedTemplates = snapshot.importedTemplates.filter((template) => template.id !== templateId);
  return {
    ...snapshot,
    customTemplates,
    importedTemplates,
    lastTemplateId: snapshot.lastTemplateId === templateId ? BUILT_IN_TEMPLATES[0].id : snapshot.lastTemplateId,
  };
}

export function templateCreateRequest(template: PdfTemplate): BlankPdfCreateRequest {
  if (template.kind !== 'generated') throw new Error('Imported templates are created from their managed PDF source.');
  return resolveBlankPdfDimensions(template.settings);
}

export function templateSummary(template: PdfTemplate): string {
  if (template.kind === 'imported-pdf') return `${template.pageCount} ${template.pageCount === 1 ? 'page' : 'pages'} · Imported PDF`;
  const request = templateCreateRequest(template);
  const orientation = request.widthMm >= request.heightMm ? 'Landscape' : 'Portrait';
  return `${request.widthMm} × ${request.heightMm} mm · ${orientation}`;
}

export function templateGridSummary(template: PdfTemplate): string {
  if (template.kind === 'imported-pdf') return 'Page grid not defined';
  const pattern = template.settings.patternType;
  if (pattern === 'blank') return 'No page grid';
  const spacing = template.settings.patternSpacingPreset === 'custom'
    ? template.settings.customPatternSpacing
    : template.settings.patternSpacingPreset;
  return `Page grid · ${spacing} mm`;
}

function migrateBlankPdfDefault(storage: TemplateStorage): TemplateLibrarySnapshot {
  const legacy = storage.getItem('butter-paper.blank-pdf-settings.v1');
  if (!legacy) return { version: 1, customTemplates: [], importedTemplates: [], lastTemplateId: BUILT_IN_TEMPLATES[0].id };
  try {
    const settings = validatedSettings(JSON.parse(legacy) as BlankPdfSettings);
    const matchingBuiltIn = BUILT_IN_TEMPLATES.find((template) => JSON.stringify(template.settings) === JSON.stringify(settings));
    if (matchingBuiltIn) return { version: 1, customTemplates: [], importedTemplates: [], lastTemplateId: matchingBuiltIn.id };
    const migrated: PdfTemplate = {
      id: 'custom-migrated-blank-pdf-default',
      name: 'Previous Blank PDF',
      kind: 'generated',
      builtIn: false,
      settings,
    };
    return { version: 1, customTemplates: [migrated], importedTemplates: [], lastTemplateId: migrated.id };
  } catch {
    return { version: 1, customTemplates: [], importedTemplates: [], lastTemplateId: BUILT_IN_TEMPLATES[0].id };
  }
}

function isValidCustomTemplate(value: unknown): value is GeneratedPdfTemplate {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Partial<PdfTemplate>;
  if (typeof candidate.id !== 'string' || !candidate.id.startsWith('custom-') || typeof candidate.name !== 'string' || candidate.name.trim().length === 0) return false;
  if (candidate.kind !== 'generated' || candidate.builtIn !== false || !candidate.settings) return false;
  try {
    validatedSettings(candidate.settings);
    return true;
  } catch {
    return false;
  }
}

function validatedSettings(settings: BlankPdfSettings): BlankPdfSettings {
  resolveBlankPdfDimensions(settings);
  return { ...settings };
}

function normalizedTemplateName(name: string): string {
  const normalized = name.trim().replace(/\s+/g, ' ');
  if (!normalized) throw new Error('Template name is required.');
  if (normalized.length > 80) throw new Error('Template name must be 80 characters or fewer.');
  return normalized;
}
