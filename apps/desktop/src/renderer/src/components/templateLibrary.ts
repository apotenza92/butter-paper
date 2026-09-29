import type {
  BlankPdfCreateRequest,
  ElectronMigrationExportRequest,
  ImportedPdfTemplateRecord,
} from '../../../shared/protocol';
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

/**
 * Reads renderer-owned migration state without the recovery/filtering used by
 * the interactive app. Any malformed or unrecognised persisted field aborts
 * the export so migration cannot silently certify data loss.
 */
export function readRendererMigrationSnapshot(
  storage: Pick<TemplateStorage, 'getItem'>,
  menuBarVisible: boolean,
): ElectronMigrationExportRequest {
  const raw = storage.getItem(TEMPLATE_LIBRARY_STORAGE_KEY);
  const snapshot = raw === null
    ? readStrictLegacyMigrationSnapshot(storage)
    : parseStrictTemplateLibrary(raw);
  return {
    menuBarVisible,
    lastTemplateId: snapshot.lastTemplateId,
    generatedTemplates: snapshot.customTemplates.map((template) => ({
      id: template.id,
      name: template.name,
      title: 'Untitled' as const,
      request: templateCreateRequest(template),
    })),
  };
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

function parseStrictTemplateLibrary(raw: string): TemplateLibrarySnapshot {
  const candidate = parseJsonRecord(raw, 'template library');
  assertExactKeys(candidate, ['version', 'customTemplates', 'lastTemplateId'], 'template library');
  if (candidate.version !== 1 || !Array.isArray(candidate.customTemplates)) {
    throw new Error('The template library migration snapshot is invalid.');
  }
  const customTemplates = candidate.customTemplates.map((value, index) => parseStrictCustomTemplate(value, index));
  const ids = new Set<string>();
  for (const template of customTemplates) {
    if (ids.has(template.id)) throw new Error('The template library contains duplicate template identifiers.');
    ids.add(template.id);
  }
  if (typeof candidate.lastTemplateId !== 'string'
    || (!BUILT_IN_TEMPLATES.some(({ id }) => id === candidate.lastTemplateId)
      && !ids.has(candidate.lastTemplateId)
      && !/^imported-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(candidate.lastTemplateId))) {
    throw new Error('The template library last selection is unavailable.');
  }
  return { version: 1, customTemplates, importedTemplates: [], lastTemplateId: candidate.lastTemplateId };
}

function readStrictLegacyMigrationSnapshot(storage: Pick<TemplateStorage, 'getItem'>): TemplateLibrarySnapshot {
  const raw = storage.getItem('butter-paper.blank-pdf-settings.v1');
  if (raw === null) {
    return { version: 1, customTemplates: [], importedTemplates: [], lastTemplateId: BUILT_IN_TEMPLATES[0].id };
  }
  const settings = parseStrictSettings(parseJsonRecord(raw, 'legacy blank PDF settings'));
  const matchingBuiltIn = BUILT_IN_TEMPLATES.find((template) => JSON.stringify(template.settings) === JSON.stringify(settings));
  if (matchingBuiltIn) {
    return { version: 1, customTemplates: [], importedTemplates: [], lastTemplateId: matchingBuiltIn.id };
  }
  const migrated: GeneratedPdfTemplate = {
    id: 'custom-migrated-blank-pdf-default',
    name: 'Previous Blank PDF',
    kind: 'generated',
    builtIn: false,
    settings,
  };
  return { version: 1, customTemplates: [migrated], importedTemplates: [], lastTemplateId: migrated.id };
}

function parseStrictCustomTemplate(value: unknown, index: number): GeneratedPdfTemplate {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`Custom template ${index + 1} is invalid.`);
  }
  const candidate = value as Record<string, unknown>;
  assertExactKeys(candidate, ['id', 'name', 'kind', 'builtIn', 'settings'], `custom template ${index + 1}`);
  if (typeof candidate.id !== 'string'
    || !/^custom-[a-z0-9_-]+$/.test(candidate.id)
    || candidate.id.length > 128
    || typeof candidate.name !== 'string'
    || candidate.name !== candidate.name.trim()
    || candidate.name.length === 0
    || [...candidate.name].some((character) => /\p{Cc}/u.test(character))
    || candidate.name.length > 80
    || candidate.kind !== 'generated'
    || candidate.builtIn !== false
    || !candidate.settings
    || typeof candidate.settings !== 'object'
    || Array.isArray(candidate.settings)) {
    throw new Error(`Custom template ${index + 1} is invalid.`);
  }
  return {
    id: candidate.id,
    name: candidate.name,
    kind: 'generated',
    builtIn: false,
    settings: parseStrictSettings(candidate.settings as Record<string, unknown>),
  };
}

function parseStrictSettings(candidate: Record<string, unknown>): BlankPdfSettings {
  assertExactKeys(candidate, [
    'preset', 'orientation', 'customWidth', 'customHeight', 'patternType',
    'patternSpacingPreset', 'customPatternSpacing', 'patternColorPreset', 'customPatternColor',
  ], 'blank PDF settings');
  const settings = candidate as unknown as BlankPdfSettings;
  const allowedPreset = ['a5', 'a4', 'a3', 'a2', 'a1', 'a0', 'custom'].includes(settings.preset);
  const allowedPattern = ['blank', 'dots', 'grid', 'lined', 'isometric', 'triangle'].includes(settings.patternType);
  if (!allowedPreset
    || !['portrait', 'landscape'].includes(settings.orientation)
    || typeof settings.customWidth !== 'string'
    || typeof settings.customHeight !== 'string'
    || !allowedPattern
    || !['5', '10', '25', 'custom'].includes(settings.patternSpacingPreset)
    || typeof settings.customPatternSpacing !== 'string'
    || !['grey', 'black', 'blue', 'custom'].includes(settings.patternColorPreset)
    || typeof settings.customPatternColor !== 'string') {
    throw new Error('Blank PDF settings are invalid.');
  }
  return validatedSettings(settings);
}

function parseJsonRecord(raw: string, label: string): Record<string, unknown> {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new Error(`The ${label} contains invalid JSON.`);
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`The ${label} is invalid.`);
  }
  return value as Record<string, unknown>;
}

function assertExactKeys(value: Record<string, unknown>, expected: readonly string[], label: string): void {
  const actual = Object.keys(value).sort();
  const required = [...expected].sort();
  if (actual.length !== required.length || actual.some((key, index) => key !== required[index])) {
    throw new Error(`The ${label} has missing or unrecognised fields.`);
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
