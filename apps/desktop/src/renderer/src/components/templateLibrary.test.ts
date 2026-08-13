import { describe, expect, it } from 'vitest';
import { DEFAULT_BLANK_PDF_SETTINGS } from './blankPdfSettings';
import {
  addGeneratedTemplate,
  allTemplates,
  BUILT_IN_TEMPLATES,
  lastTemplate,
  loadTemplateLibrary,
  removeTemplate,
  saveTemplateLibrary,
  useTemplate,
  withImportedTemplates,
} from './templateLibrary';

function storage(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial));
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
    value: (key: string) => values.get(key),
  };
}

describe('template library', () => {
  it('provides built-in paper templates and remembers the last one', () => {
    const memory = storage();
    const initial = loadTemplateLibrary(memory);
    const selected = useTemplate(initial, 'built-in-grid');
    saveTemplateLibrary(memory, selected);

    expect(BUILT_IN_TEMPLATES.map((template) => template.name)).toEqual([
      'Blank Paper', 'Dot Grid', 'Square Grid', 'Ruled Paper', 'Isometric Grid', 'Triangle Grid',
    ]);
    expect(lastTemplate(loadTemplateLibrary(memory)).name).toBe('Square Grid');
  });

  it('creates and removes a validated custom paper template', () => {
    const initial = loadTemplateLibrary(storage());
    const added = addGeneratedTemplate(initial, '  My   Grid  ', { ...DEFAULT_BLANK_PDF_SETTINGS, patternType: 'grid' }, 'fixed-id');
    expect(lastTemplate(added).name).toBe('My Grid');
    expect(removeTemplate(added, 'custom-fixed-id').lastTemplateId).toBe('built-in-blank');
  });

  it('merges managed imported PDFs without persisting their paths in renderer storage', () => {
    const memory = storage();
    const initial = loadTemplateLibrary(memory);
    const merged = withImportedTemplates(initial, [{
      id: 'imported-00000000-0000-4000-8000-000000000000',
      name: 'Site Form',
      kind: 'imported-pdf',
      pageCount: 3,
      createdAt: '2026-08-13T00:00:00.000Z',
    }]);
    saveTemplateLibrary(memory, { ...merged, lastTemplateId: merged.importedTemplates[0].id });

    expect(allTemplates(merged).at(-1)).toMatchObject({ name: 'Site Form', pageCount: 3 });
    expect(memory.value('butter-paper.template-library.v1')).not.toContain('Site Form');
    const reloaded = loadTemplateLibrary(memory);
    expect(reloaded.lastTemplateId).toBe('imported-00000000-0000-4000-8000-000000000000');
    expect(lastTemplate(withImportedTemplates(reloaded, merged.importedTemplates)).name).toBe('Site Form');
  });
});
