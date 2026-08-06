import { describe, expect, it } from 'vitest';
import {
  assessDeletion,
  assessMutation,
  createMutationPlans,
  markupDictionary,
  markupSetCommand,
  parseBluebeamDictionary,
  parseCliOptions,
  parseInventoryOutput,
  quoteBciString,
} from '../../scripts/bluebeam-compat/revu-script-matrix.mjs';

describe('Bluebeam ScriptEngine compatibility matrix', () => {
  it('parses the fixed lane CLI without allowing ambiguous positional paths', () => {
    expect(parseCliOptions(['--pdf', 'fixture.pdf', '--output', 'evidence', '--vm', 'Windows 11'], {})).toMatchObject({
      pdfPath: 'fixture.pdf', outputDirectory: 'evidence', vmName: 'Windows 11',
    });
    expect(() => parseCliOptions(['fixture.pdf'], {})).toThrow(/Unknown option/);
    expect(() => parseCliOptions(['--pdf', 'fixture.pdf'], {})).toThrow(/--output/);
  });

  it('quotes official BCI strings and nested MarkupSet dictionaries deterministically', () => {
    expect(quoteBciString('a"b|c\nd')).toBe('"a|"b||c|nd"');
    expect(markupDictionary({ x: '55', comment: 'safe' })).toBe('{"x":"55","comment":"safe"}');
    expect(markupSetCommand(2, 'bp:compat-rectangle', { x: '55' })).toBe('MarkupSet(2,"bp:compat-rectangle","{|' + '"x|' + '":|' + '"55|' + '"}")');
  });

  it('parses pipe-escaped nested MarkupGetExList dictionaries', () => {
    const raw = "{'bp:one':'{|'type|':|'FreeText|',|'comment|':|'line 1||rline 2|'}'}";
    expect(parseBluebeamDictionary(raw)).toEqual({ 'bp:one': "{'type':'FreeText','comment':'line 1|rline 2'}" });
    expect(parseInventoryOutput(`1\r\n${raw}\r\n`)).toEqual({ 'bp:one': { type: 'FreeText', comment: 'line 1\rline 2' } });
  });

  it('marks only properties exposed by ScriptEngine as planned', () => {
    const inventory = {
      'bp:compat-text-box': { x: '40', y: '102', width: '150', height: '48', rotation: '0', comment: '', color: '#FF0000', opacity: '1', linewidth: '0', linestyle: 'solid' },
      'bp:compat-arrow': { comment: '', color: '#FF0000', opacity: '1', linewidth: '0.5', linestyle: 'solid' },
    };
    const plans = createMutationPlans(inventory);
    expect(plans.find((plan) => plan.id === 'move')?.tools.find((tool) => tool.tool === 'text-box')).toMatchObject({ status: 'planned', properties: { x: '50', y: '112' } });
    expect(plans.find((plan) => plan.id === 'move')?.tools.find((tool) => tool.tool === 'arrow')).toMatchObject({ status: 'unsupported', reason: 'ScriptEngine-property-not-exposed:x,y' });
    expect(plans.find((plan) => plan.id === 'resize')?.tools.find((tool) => tool.tool === 'arrow')).toMatchObject({ status: 'unsupported', reason: 'operation-not-in-tool-contract' });
  });

  it('distinguishes exact, normalized/partial, and unchanged/unsupported MarkupSet results', () => {
    const plan = {
      id: 'restyle',
      tools: [
        { tool: 'rectangle', status: 'planned', targetId: 'r', properties: { color: '#0080FF', opacity: '0.65' } },
        { tool: 'line', status: 'planned', targetId: 'l', properties: { color: '#0080FF', opacity: '0.65' } },
        { tool: 'arc', status: 'unsupported', reason: 'not-exposed' },
      ],
    };
    const before = { r: { color: '#FF0000', opacity: '1' }, l: { color: '#FF0000', opacity: '1' } };
    expect(assessMutation(plan, before, { r: { color: '#0080ff', opacity: '0.6500' }, l: { color: '#0080FF', opacity: '1' } })).toEqual([
      expect.objectContaining({ tool: 'rectangle', status: 'passed' }),
      expect.objectContaining({ tool: 'line', status: 'partial', changedKeys: ['color'] }),
      expect.objectContaining({ tool: 'arc', status: 'unsupported' }),
    ]);
  });

  it('requires every native component of a logical Cloud+ to disappear for deletion to pass', () => {
    const before = { 'bp:compat-cloud-plus:cloud': {}, 'bp:compat-cloud-plus:text': {}, 'bp:compat-rectangle': {} };
    const result = assessDeletion(before, { 'bp:compat-cloud-plus:cloud': {} });
    expect(result.find((item) => item.tool === 'cloud-plus')).toMatchObject({ status: 'partial', removed: ['bp:compat-cloud-plus:text'] });
    expect(result.find((item) => item.tool === 'rectangle')).toMatchObject({ status: 'passed' });
  });
});
