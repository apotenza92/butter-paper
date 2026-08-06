import { describe, expect, it } from 'vitest';
import { expectedComponents, loadToolContract, validateInspectedComponents, validateOperationResults, validateToolContract } from '../../scripts/bluebeam-compat/tool-contract.mjs';

describe('Bluebeam per-tool executable contract', () => {
  it('covers Select/Pan and all 19 current annotation tools', async () => {
    const contract = await loadToolContract();
    expect(contract.tools).toHaveLength(21);
    expect(contract.tools.filter((tool: { kind: string }) => tool.kind === 'annotation')).toHaveLength(19);
    expect(contract.tools.find((tool: { id: string }) => tool.id === 'select').operations).toEqual(['activate', 'single-select', 'multi-select', 'move-selection', 'no-create']);
    expect(contract.tools.find((tool: { id: string }) => tool.id === 'pan').operations).toEqual(['activate', 'navigate', 'no-selection', 'no-mutation', 'no-create']);
    expect(contract.tools.find((tool: { id: string }) => tool.id === 'cloud-plus')).toMatchObject({ componentCount: 2 });
    expect(contract.compatibilityPolicy).toMatchObject({
      untouchedImported: 'verbatim-native-components',
      editedImported: 'canonical-native-replacement',
      deletedImported: 'remove-all-logical-components',
      safeImprovements: 'persist-only-standard-native-geometry',
    });
    expect(contract.compatibilityPolicy.unsupportedEditedProperties).toEqual(expect.arrayContaining([
      expect.objectContaining({ fields: expect.arrayContaining(['custom-dash-pattern', 'custom-line-ending']) }),
      expect.objectContaining({ fields: ['unsupported-native-media-encoding'] }),
    ]));
    expect(expectedComponents(contract, ['cloud-plus'])).toEqual([
      { tool: 'cloud-plus', role: 'cloud', subtype: 'Polygon', intent: 'PolygonCloud' },
      { tool: 'cloud-plus', role: 'text', subtype: 'FreeText', intent: 'FreeTextCallout' },
    ]);
  });

  it('rejects incomplete component and operation coverage', async () => {
    const contract = await loadToolContract();
    const invalid = structuredClone(contract);
    invalid.tools.find((tool: { id: string }) => tool.id === 'cloud-plus').componentCount = 1;
    expect(() => validateToolContract(invalid)).toThrowError(/Component count mismatch/);
    const invalidPolicy = structuredClone(contract);
    invalidPolicy.compatibilityPolicy.safeImprovements = 'private-butter-paper-metadata';
    expect(() => validateToolContract(invalidPolicy)).toThrowError(/Invalid compatibility policy/);
    const empty = validateOperationResults(contract, []);
    expect(empty.passed).toBe(false);
    expect(empty.missing).toContain('cloud-plus:text-edit');
  });

  it('requires every declared operation to pass', async () => {
    const contract = await loadToolContract();
    const results = contract.tools.flatMap((tool: { id: string; operations: string[] }) => tool.operations.map((operation) => ({ tool: tool.id, operation, status: 'passed' })));
    expect(validateOperationResults(contract, results)).toEqual({ passed: true, missing: [], failed: [] });
    results.find((result: { tool: string; operation: string; status: string }) => result.tool === 'cloud' && result.operation === 'reshape').status = 'failed';
    expect(validateOperationResults(contract, results).failed).toEqual(['cloud:reshape']);
  });

  it('gates inspected native subtype, intent, component count, measure, and blend-mode contracts', async () => {
    const contract = await loadToolContract();
    const inspection = {
      pages: [{ annotations: [
        { subtype: 'Polygon', intent: 'PolygonCloud', name: 'cloud' },
        { subtype: 'FreeText', intent: 'FreeTextCallout', name: 'text' },
        { subtype: 'Ink', intent: null, blendMode: 'Multiply', name: 'highlight' },
        { subtype: 'Line', intent: 'LineDimension', measure: true, name: 'length' },
      ] }],
    };

    expect(validateInspectedComponents(contract, ['cloud-plus', 'highlight', 'length'], inspection)).toEqual({
      passed: true,
      missing: [],
      unexpected: [],
    });
    const wrongIntent = structuredClone(inspection);
    wrongIntent.pages[0].annotations[1].intent = null;
    expect(validateInspectedComponents(contract, ['cloud-plus', 'highlight', 'length'], wrongIntent)).toMatchObject({
      passed: false,
      missing: [expect.stringContaining('cloud-plus:text')],
    });
  });
});
