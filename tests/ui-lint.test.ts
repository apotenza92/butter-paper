import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { expect, it } from 'vitest';

it('checks Nova composition and theme discovery while preserving dynamic PDF geometry and ink', () => {
  const cwd = resolve('apps/desktop');
  const output = join(cwd, 'test-results');
  mkdirSync(output, { recursive: true });
  const directory = mkdtempSync(join(output, 'ui-lint-'));
  const file = join(directory, 'probe.tsx');
  try {
    writeFileSync(file, [
      'import { Button } from "@/components/ui/button";',
      'export const valid = <Button size="sm" variant="outline" className="mt-4 w-full">Save</Button>;',
      'export const theme = <div className="bg-background text-muted-foreground" />;',
      'export const restyled = <Button className="p-4">Save</Button>;',
      'export const raw = <div className="bg-pink-500" />;',
      'export const unknown = <div className="rounded-huge" />;',
      'export const geometry = (x: number, ink: string) => <svg style={{ left: x }}><path stroke={ink} /></svg>;',
      'export const icon = <svg className="size-4" stroke="currentColor" />;',
      'export const notRail = <Button className="p-0">Unapproved override</Button>;',
    ].join('\n'));
    const execution = spawnSync('pnpm', [
      'exec', 'oxlint', '-c', '.oxlintrc.json', '--no-ignore', '--format', 'json', file,
    ], { cwd, encoding: 'utf8', timeout: 30_000 });
    expect(execution.status).toBe(1);
    const result = JSON.parse(execution.stdout);
    const findings = result.diagnostics.map((diagnostic: {
      code: string; labels: { span: { line: number } }[];
    }) => [diagnostic.code, diagnostic.labels[0].span.line]);
    expect(findings).toEqual(expect.arrayContaining([
      ['shadcn(no-restyle)', 4],
      ['shadcn(no-raw-colors)', 5],
      ['shadcn(no-unknown-classes)', 6],
      ['shadcn(no-restyle)', 9],
    ]));
    expect(findings).toHaveLength(4);
    expect(result.diagnostics.find((d: { code: string }) => d.code === 'shadcn(no-restyle)').message)
      .toContain('default, xs, sm, lg, icon, icon-xs, icon-sm, icon-lg');
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}, 40_000);
