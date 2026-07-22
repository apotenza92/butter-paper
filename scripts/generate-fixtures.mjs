import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const moduleDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(moduleDir, '..');
const outputDir = join(repoRoot, 'tests/fixtures/generated');
const generatedAt = '2024-01-01T00:00:00.000Z';

mkdirSync(outputDir, { recursive: true });

const fixtures = [
  {
    name: 'single-page',
    file: 'single-page.pdf',
    pages: [
      textPage(612, 792, [
        ['Butter Paper fixture', 54, 720, 24],
        ['Single page inspection sample', 54, 690, 14],
      ]),
    ],
  },
  {
    name: 'zoom-target',
    file: 'zoom-target.pdf',
    pages: [
      textPage(612, 792, [
        ['Butter Paper zoom target', 54, 720, 24],
        ['Markup workflow sample', 54, 690, 14],
      ]),
    ],
  },
  {
    name: 'multi-page',
    file: 'multi-page.pdf',
    pages: Array.from({ length: 6 }, (_, index) => textPage(612, 792, [
      [`Multi-page fixture`, 54, 720, 24],
      [`Page ${index + 1} of 6`, 54, 690, 14],
    ])),
  },
  {
    name: 'engineering-large',
    file: 'engineering-large.pdf',
    pages: [
      engineeringPage(1584, 1224),
    ],
  },
  {
    name: 'rotated-page',
    file: 'rotated-page.pdf',
    pages: [
      textPage(842, 595, [
        ['Rotated page fixture', 54, 520, 24],
        ['Page media box is landscape with /Rotate 90', 54, 490, 14],
      ], { rotation: 90 }),
    ],
  },
  {
    name: 'mixed-page-sizes',
    file: 'mixed-page-sizes.pdf',
    pages: [
      textPage(595, 842, [['Mixed sizes fixture', 54, 780, 24], ['A4 page', 54, 748, 14]]),
      textPage(842, 1191, [['Mixed sizes fixture', 54, 1120, 24], ['A3 page', 54, 1088, 14]]),
      textPage(1191, 1684, [['Mixed sizes fixture', 54, 1600, 24], ['A2 page', 54, 1568, 14]]),
    ],
  },
];

const manifest = {
  generatedAt,
  fixtures: fixtures.map((fixture) => ({
    name: fixture.name,
    file: fixture.file,
    pageCount: fixture.pages.length,
    pages: fixture.pages.map((page) => ({
      width: page.width,
      height: page.height,
      rotation: page.rotation ?? 0,
    })),
  })),
};

for (const fixture of fixtures) {
  const pdf = buildPdf(fixture.name, fixture.pages);
  writeFileSync(join(outputDir, fixture.file), pdf);
}

writeFileSync(join(outputDir, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);

function textPage(width, height, textLines, options = {}) {
  return {
    width,
    height,
    rotation: options.rotation ?? 0,
    content: buildTextContent(width, height, textLines, options.rotation ?? 0),
  };
}

function engineeringPage(width, height) {
  const commands = [];
  commands.push('0.92 0.92 0.92 RG');
  commands.push('0.35 w');

  const minorStep = 36;
  const majorStep = 144;

  for (let x = 0; x <= width; x += minorStep) {
    commands.push(`${lineColour(x, majorStep)}${x} 0 m ${x} ${height} l S`);
  }

  for (let y = 0; y <= height; y += minorStep) {
    commands.push(`${lineColour(y, majorStep)}0 ${y} m ${width} ${y} l S`);
  }

  commands.push('0.15 0.15 0.15 RG');
  commands.push('1.25 w');
  commands.push(`24 24 ${width - 48} ${height - 48} re S`);

  commands.push('0 0 0 RG');
  commands.push('BT');
  commands.push('/F1 24 Tf');
  commands.push(`1 0 0 1 54 ${height - 72} Tm`);
  commands.push('(Butter Paper engineering sheet) Tj');
  commands.push('ET');

  commands.push('BT');
  commands.push('/F1 14 Tf');
  commands.push(`1 0 0 1 54 ${height - 102} Tm`);
  commands.push('(Grid spacing: 36pt minor, 144pt major) Tj');
  commands.push('ET');

  const titleBlockWidth = 420;
  const titleBlockHeight = 120;
  const titleBlockX = width - titleBlockWidth - 36;
  const titleBlockY = 36;

  commands.push('0.1 0.1 0.1 RG');
  commands.push('1 w');
  commands.push(`${titleBlockX} ${titleBlockY} ${titleBlockWidth} ${titleBlockHeight} re S`);
  commands.push(`${titleBlockX} ${titleBlockY + 64} ${titleBlockWidth} 0 re S`);
  commands.push(`${titleBlockX + 160} ${titleBlockY} 0 ${titleBlockHeight} re S`);
  commands.push(`${titleBlockX + 280} ${titleBlockY} 0 ${titleBlockHeight} re S`);

  commands.push('BT');
  commands.push('/F1 12 Tf');
  commands.push(`1 0 0 1 ${titleBlockX + 12} ${titleBlockY + 86} Tm`);
  commands.push('(Sheet title) Tj');
  commands.push('ET');

  commands.push('BT');
  commands.push('/F1 16 Tf');
  commands.push(`1 0 0 1 ${titleBlockX + 12} ${titleBlockY + 44} Tm`);
  commands.push('(Typical engineering layout) Tj');
  commands.push('ET');

  commands.push('BT');
  commands.push('/F1 12 Tf');
  commands.push(`1 0 0 1 ${titleBlockX + 172} ${titleBlockY + 86} Tm`);
  commands.push('(Revision) Tj');
  commands.push('ET');

  commands.push('BT');
  commands.push('/F1 12 Tf');
  commands.push(`1 0 0 1 ${titleBlockX + 292} ${titleBlockY + 86} Tm`);
  commands.push('(Scale) Tj');
  commands.push('ET');

  commands.push('BT');
  commands.push('/F1 12 Tf');
  commands.push(`1 0 0 1 ${titleBlockX + 172} ${titleBlockY + 44} Tm`);
  commands.push('(A) Tj');
  commands.push('ET');

  commands.push('BT');
  commands.push('/F1 12 Tf');
  commands.push(`1 0 0 1 ${titleBlockX + 292} ${titleBlockY + 44} Tm`);
  commands.push('(1:50) Tj');
  commands.push('ET');

  return {
    width,
    height,
    rotation: 0,
    content: commands.join('\n'),
  };
}

function buildTextContent(width, height, textLines, rotation) {
  const commands = [];
  commands.push('0 0 0 RG');
  commands.push('1 w');
  commands.push(`24 24 ${width - 48} ${height - 48} re S`);
  commands.push('BT');

  for (const [label, x, y, size] of textLines) {
    commands.push(`/F1 ${size} Tf`);
    commands.push(`1 0 0 1 ${x} ${y} Tm`);
    commands.push(`(${escapePdfString(label)}) Tj`);
  }

  if (rotation) {
    commands.push('/F1 12 Tf');
    commands.push(`1 0 0 1 54 54 Tm`);
    commands.push(`(${escapePdfString(`Rotation ${rotation}`)}) Tj`);
  }

  commands.push('ET');
  return commands.join('\n');
}

function lineColour(value, majorStep) {
  if (value % majorStep === 0) {
    return '0.72 0.72 0.72 RG\n0.8 w\n';
  }

  return '0.88 0.88 0.88 RG\n0.35 w\n';
}

function buildPdf(name, pages) {
  const objects = [];
  const catalogId = 1;
  const pagesId = 2;
  const fontId = 3;
  const infoId = 4;
  const pageIds = pages.map((_, index) => 6 + index * 2);
  const contentIds = pages.map((_, index) => 5 + index * 2);

  objects[catalogId] = pdfObject(catalogId, `<< /Type /Catalog /Pages ${pagesId} 0 R >>`);
  objects[pagesId] = pdfObject(pagesId, `<< /Type /Pages /Kids [${pageIds.map((id) => `${id} 0 R`).join(' ')}] /Count ${pages.length} >>`);
  objects[fontId] = pdfObject(fontId, `<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>`);
  objects[infoId] = pdfObject(infoId, `<< /Title (${escapePdfString(`Butter Paper ${name}`)}) /Producer (Butter Paper fixture generator) /Creator (Butter Paper) >>`);

  pages.forEach((page, index) => {
    const contentId = contentIds[index];
    const pageId = pageIds[index];
    objects[contentId] = pdfStreamObject(contentId, page.content);
    objects[pageId] = pdfObject(pageId, `<< /Type /Page /Parent ${pagesId} 0 R /MediaBox [0 0 ${page.width} ${page.height}] /Resources << /Font << /F1 ${fontId} 0 R >> >> /Contents ${contentId} 0 R${page.rotation ? ` /Rotate ${page.rotation}` : ''} >>`);
  });

  return serializePdf(objects, infoId);
}

function pdfObject(id, body) {
  return { id, body };
}

function pdfStreamObject(id, content) {
  return {
    id,
    body: `<< /Length ${Buffer.byteLength(content, 'ascii')} >>\nstream\n${content}\nendstream`,
  };
}

function serializePdf(objects, infoId) {
  const header = '%PDF-1.7\n';
  const ordered = objects.filter(Boolean).sort((a, b) => a.id - b.id);
  const chunks = [header];
  const offsets = new Array(ordered.length + 1).fill(0);
  let offset = Buffer.byteLength(header, 'ascii');

  for (const object of ordered) {
    offsets[object.id] = offset;
    const chunk = `${object.id} 0 obj\n${object.body}\nendobj\n`;
    chunks.push(chunk);
    offset += Buffer.byteLength(chunk, 'ascii');
  }

  const xrefOffset = offset;
  const maxObjectId = ordered[ordered.length - 1].id;
  let xref = `xref\n0 ${maxObjectId + 1}\n0000000000 65535 f \n`;

  for (let id = 1; id <= maxObjectId; id += 1) {
    const objectOffset = offsets[id] ?? 0;
    xref += `${String(objectOffset).padStart(10, '0')} 00000 n \n`;
  }

  const trailer = `trailer\n<< /Size ${maxObjectId + 1} /Root 1 0 R /Info ${infoId} 0 R >>\nstartxref\n${xrefOffset}\n%%EOF\n`;

  chunks.push(xref, trailer);
  return Buffer.from(chunks.join(''), 'ascii');
}

function escapePdfString(value) {
  return value
    .replaceAll('\\', '\\\\')
    .replaceAll('(', '\\(')
    .replaceAll(')', '\\)')
    .replaceAll('\r', ' ')
    .replaceAll('\n', ' ');
}
