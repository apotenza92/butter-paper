import type { PdfInspection } from './inspect.js';

export function formatInspection(inspection: PdfInspection) {
  const lines = [
    `Path: ${inspection.path}`,
    `Title: ${inspection.metadata.title ?? 'n/a'}`,
    `Pages: ${inspection.pageCount}`,
  ];

  for (const page of inspection.pages) {
    lines.push(`Page ${page.pageNumber}: ${page.width} x ${page.height}${page.rotation ? `, rotation ${page.rotation}` : ''}`);
  }

  return lines.join('\n');
}
