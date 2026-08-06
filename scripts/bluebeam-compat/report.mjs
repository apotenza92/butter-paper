import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

export async function writeCompatibilityReports(outputDirectory, report) {
  await mkdir(outputDirectory, { recursive: true });
  await Promise.all([
    writeFile(join(outputDirectory, 'report.json'), `${JSON.stringify(report, null, 2)}\n`),
    writeFile(join(outputDirectory, 'report.junit.xml'), junit(report)),
    writeFile(join(outputDirectory, 'report.html'), html(report)),
  ]);
}

function junit(report) {
  const specimens = report.specimens ?? [];
  const failures = specimens.filter((item) => !item.passed).length;
  return `<?xml version="1.0" encoding="UTF-8"?>\n<testsuite name="Bluebeam compatibility" tests="${specimens.length}" failures="${failures}">\n${specimens.map((item) => `  <testcase name="${xml(item.id)}">${item.passed ? '' : `<failure message="${xml(item.failure ?? 'threshold exceeded')}">${xml(JSON.stringify(item.metrics ?? {}))}</failure>`}</testcase>`).join('\n')}\n</testsuite>\n`;
}

function html(report) {
  const rows = (report.specimens ?? []).map((item) => {
    const links = Object.entries(item.artifacts ?? {}).map(([label, href]) => `<a href="${xml(href)}">${xml(label)}</a>`).join(' ');
    return `<tr class="${item.passed ? 'pass' : 'fail'}"><td>${xml(item.id)}</td><td>${item.passed ? 'PASS' : 'FAIL'}${item.failure ? `<div>${xml(item.failure)}</div>` : ''}</td><td><pre>${xml(JSON.stringify(item.metrics ?? {}, null, 2))}</pre></td><td>${links}</td></tr>`;
  }).join('');
  return `<!doctype html><meta charset="utf-8"><title>Bluebeam compatibility</title><style>body{font:14px system-ui;margin:2rem}table{border-collapse:collapse;width:100%}td,th{border:1px solid #bbb;padding:.5rem;text-align:left;vertical-align:top}.pass{background:#e8f7ec}.fail{background:#fdeaea}pre{margin:0;white-space:pre-wrap}a{margin-right:.5rem}</style><h1>Bluebeam compatibility</h1><p>${xml(report.summary ?? '')}</p><table><thead><tr><th>Specimen</th><th>Result</th><th>Metrics</th><th>Artifacts</th></tr></thead><tbody>${rows}</tbody></table>`;
}

function xml(value) {
  return String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
}
