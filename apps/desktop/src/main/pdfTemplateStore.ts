import { randomUUID } from 'node:crypto';
import { mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import type { ImportedPdfTemplateRecord } from '../shared/protocol';

interface StoredTemplate extends ImportedPdfTemplateRecord {
  readonly sourceFileName: 'source.pdf';
}

interface TemplateIndex {
  readonly version: 1;
  readonly templates: readonly StoredTemplate[];
}

export class PdfTemplateStore {
  private readonly root: string;
  private readonly indexPath: string;

  constructor(userDataPath: string) {
    this.root = join(userDataPath, 'templates');
    this.indexPath = join(this.root, 'library.json');
  }

  async list(): Promise<readonly ImportedPdfTemplateRecord[]> {
    return (await this.readIndex()).templates.map(publicRecord);
  }

  async importPdf(sourcePath: string): Promise<ImportedPdfTemplateRecord> {
    const bytes = await readFile(sourcePath);
    return this.importBytes(bytes, templateNameFromPath(sourcePath));
  }

  async importBytes(bytes: Uint8Array, name: string): Promise<ImportedPdfTemplateRecord> {
    const { PDFDocument } = await import('pdf-lib');
    const document = await PDFDocument.load(bytes, { updateMetadata: false });
    if (document.getPageCount() < 1) throw new Error('The template PDF has no pages.');

    const id = `imported-${randomUUID()}`;
    const templateDirectory = join(this.root, id);
    const record: StoredTemplate = {
      id,
      name: normalizedTemplateName(name),
      kind: 'imported-pdf',
      pageCount: document.getPageCount(),
      createdAt: new Date().toISOString(),
      sourceFileName: 'source.pdf',
    };
    await mkdir(templateDirectory, { recursive: true });
    try {
      await writeFile(join(templateDirectory, record.sourceFileName), bytes, { mode: 0o600 });
      const index = await this.readIndex();
      await this.writeIndex({ ...index, templates: [...index.templates, record] });
      return publicRecord(record);
    } catch (error) {
      await rm(templateDirectory, { recursive: true, force: true });
      throw error;
    }
  }

  async remove(templateId: string): Promise<void> {
    assertTemplateId(templateId);
    const index = await this.readIndex();
    if (!index.templates.some((template) => template.id === templateId)) return;
    await this.writeIndex({ ...index, templates: index.templates.filter((template) => template.id !== templateId) });
    await rm(join(this.root, templateId), { recursive: true, force: true });
  }

  async readSource(templateId: string): Promise<Uint8Array> {
    assertTemplateId(templateId);
    const template = (await this.readIndex()).templates.find((candidate) => candidate.id === templateId);
    if (!template) throw new Error('The PDF template no longer exists.');
    return readFile(join(this.root, template.id, template.sourceFileName));
  }

  private async readIndex(): Promise<TemplateIndex> {
    try {
      const parsed = JSON.parse(await readFile(this.indexPath, 'utf8')) as Partial<TemplateIndex>;
      if (parsed.version !== 1 || !Array.isArray(parsed.templates)) return { version: 1, templates: [] };
      return { version: 1, templates: parsed.templates.filter(isStoredTemplate) };
    } catch (error) {
      if (isFileSystemError(error, 'ENOENT') || error instanceof SyntaxError) return { version: 1, templates: [] };
      throw error;
    }
  }

  private async writeIndex(index: TemplateIndex): Promise<void> {
    await mkdir(this.root, { recursive: true });
    const temporaryPath = `${this.indexPath}.tmp-${randomUUID()}`;
    await writeFile(temporaryPath, `${JSON.stringify(index, null, 2)}\n`, { encoding: 'utf8', mode: 0o600 });
    await rename(temporaryPath, this.indexPath);
  }
}

function publicRecord(template: StoredTemplate): ImportedPdfTemplateRecord {
  const { sourceFileName: _sourceFileName, ...record } = template;
  return record;
}

function templateNameFromPath(path: string): string {
  return basename(path).replace(/\.pdf$/i, '').trim() || 'Imported PDF';
}

function normalizedTemplateName(value: string): string {
  const name = value.replace(/\.pdf$/i, '').trim().replace(/\s+/g, ' ');
  if (!name) return 'Imported PDF';
  return name.slice(0, 80);
}

function assertTemplateId(value: string): void {
  if (!/^imported-[0-9a-f-]{36}$/i.test(value)) throw new TypeError('Template identifier is invalid.');
}

function isStoredTemplate(value: unknown): value is StoredTemplate {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Partial<StoredTemplate>;
  return typeof candidate.id === 'string'
    && /^imported-[0-9a-f-]{36}$/i.test(candidate.id)
    && typeof candidate.name === 'string'
    && candidate.name.length > 0
    && candidate.kind === 'imported-pdf'
    && Number.isInteger(candidate.pageCount)
    && candidate.pageCount! > 0
    && typeof candidate.createdAt === 'string'
    && candidate.sourceFileName === 'source.pdf';
}

function isFileSystemError(error: unknown, code: string): error is NodeJS.ErrnoException {
  return error instanceof Error && 'code' in error && error.code === code;
}
