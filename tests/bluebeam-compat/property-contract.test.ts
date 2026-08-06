import { readFile } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import {
  bluebeamPropertyContract,
  renderPropertyContractMarkdown,
  validatePropertyContract,
} from '../../scripts/bluebeam-compat/property-contract.mjs';

describe('Bluebeam property contract', () => {
  it('audits the 19 annotation tools and Count without adding navigation tools', () => {
    expect(validatePropertyContract()).toBe(bluebeamPropertyContract);
    expect(bluebeamPropertyContract.tools).toHaveLength(20);
    expect(bluebeamPropertyContract.tools.filter((tool) => tool.butterPaperImplemented)).toHaveLength(19);
    expect(bluebeamPropertyContract.tools.find((tool) => tool.id === 'count')).toMatchObject({ butterPaperImplemented: false });
    expect(bluebeamPropertyContract.tools.map((tool) => tool.id)).not.toContain('select');
    expect(bluebeamPropertyContract.tools.map((tool) => tool.id)).not.toContain('pan');
  });

  it('keeps every scoped key unique with citations, evidence, conditions, PDF fields, and statuses', () => {
    const properties = bluebeamPropertyContract.tools.flatMap((tool) => tool.properties);
    const keys = properties.map((property) => `${property.tool}:${property.key}`);
    expect(new Set(keys).size).toBe(keys.length);
    expect(properties.every((property) => property.sourceRefs.length > 0 && property.evidenceRefs.length > 0)).toBe(true);
    expect(properties.every((property) => property.pdf.appearanceStream && property.pdf.groupedAnnotations)).toBe(true);
  });

  it('keeps the tracked Markdown report synchronized', async () => {
    const report = await readFile(new URL('../../docs/compatibility/bluebeam-properties.md', import.meta.url), 'utf8');
    expect(report).toBe(renderPropertyContractMarkdown());
  });
});
