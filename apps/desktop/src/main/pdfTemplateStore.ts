import { randomUUID } from 'node:crypto';
import { constants } from 'node:fs';
import { lstat, mkdir, open, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import type { ImportedPdfTemplateRecord } from '../shared/protocol';

interface StoredTemplate extends ImportedPdfTemplateRecord {
  readonly sourceFileName: 'source.pdf';
}

interface TemplateIndex {
  readonly version: 1;
  readonly templates: readonly StoredTemplate[];
}

export interface PdfTemplateMigrationSnapshot {
  readonly records: readonly ImportedPdfTemplateRecord[];
  readonly sources: readonly { readonly id: string; readonly bytes: Uint8Array }[];
}

const MAX_MIGRATION_INDEX_BYTES = 1024 * 1024;
const MAX_MIGRATION_SOURCE_BYTES = 256 * 1024 * 1024;
const MAX_MIGRATION_TOTAL_SOURCE_BYTES = 1024 * 1024 * 1024;
const MAX_MIGRATION_TEMPLATES = 256;

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

  /**
   * Migration-only snapshot. Unlike the interactive loader, this fails closed
   * on malformed metadata or storage drift and returns owned bytes, never
   * privileged filesystem paths.
   */
  async snapshotForMigration(): Promise<PdfTemplateMigrationSnapshot> {
    try {
      const metadata = await lstat(this.root);
      if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
        throw new Error('The imported template store is not a private directory.');
      }
    } catch (error) {
      if (isFileSystemError(error, 'ENOENT')) return { records: [], sources: [] };
      throw error;
    }
    const index = await this.readStrictMigrationIndex();
    if (index.templates.length > MAX_MIGRATION_TEMPLATES) {
      throw new Error('The imported template migration snapshot is too large.');
    }

    const rootEntries = (await readdir(this.root)).sort();
    const expectedRootEntries = ['library.json', ...index.templates.map(({ id }) => id)].sort();
    if (!sameStrings(rootEntries, expectedRootEntries)) {
      throw new Error('The imported template store contains unrecognised entries.');
    }

    let totalBytes = 0;
    const sources: Array<{ id: string; bytes: Uint8Array }> = [];
    const { PDFDocument } = await import('pdf-lib');
    for (const template of index.templates) {
      await assertPrivateDirectory(join(this.root, template.id), 'imported template directory');
      const entries = await readdir(join(this.root, template.id));
      if (!sameStrings(entries, [template.sourceFileName])) {
        throw new Error(`Imported template ${template.id} contains unrecognised files.`);
      }
      const bytes = await readPrivateRegularFile(
        join(this.root, template.id, template.sourceFileName),
        MAX_MIGRATION_SOURCE_BYTES,
      );
      totalBytes += bytes.byteLength;
      if (totalBytes > MAX_MIGRATION_TOTAL_SOURCE_BYTES) {
        throw new Error('The imported template migration snapshot is too large.');
      }
      let pageCount: number;
      try {
        pageCount = (await PDFDocument.load(bytes, { updateMetadata: false })).getPageCount();
      } catch {
        throw new Error(`Imported template ${template.id} is not a valid PDF.`);
      }
      if (pageCount !== template.pageCount) {
        throw new Error(`Imported template ${template.id} page count changed.`);
      }
      sources.push({ id: template.id, bytes });
    }
    return { records: index.templates.map(publicRecord), sources };
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

  private async readStrictMigrationIndex(): Promise<TemplateIndex> {
    const bytes = await readPrivateRegularFile(this.indexPath, MAX_MIGRATION_INDEX_BYTES);
    let value: unknown;
    try {
      value = JSON.parse(Buffer.from(bytes).toString('utf8'));
    } catch {
      throw new Error('The imported template index contains invalid JSON.');
    }
    if (!isRecord(value) || !hasExactKeys(value, ['version', 'templates']) || value.version !== 1 || !Array.isArray(value.templates)) {
      throw new Error('The imported template index is invalid.');
    }
    const templates = value.templates.map((candidate, index) => parseStrictStoredTemplate(candidate, index));
    const identifiers = new Set(templates.map(({ id }) => id));
    if (identifiers.size !== templates.length) {
      throw new Error('The imported template index contains duplicate identifiers.');
    }
    await assertPrivateDirectory(this.root, 'imported template store');
    return { version: 1, templates };
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

function parseStrictStoredTemplate(value: unknown, index: number): StoredTemplate {
  if (!isRecord(value)
    || !hasExactKeys(value, ['id', 'name', 'kind', 'pageCount', 'createdAt', 'sourceFileName'])
    || typeof value.id !== 'string'
    || !/^imported-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value.id)
    || typeof value.name !== 'string'
    || value.name.length === 0
    || value.name !== value.name.trim()
    || value.name.length > 80
    || [...value.name].some((character) => /\p{Cc}/u.test(character))
    || value.kind !== 'imported-pdf'
    || !Number.isSafeInteger(value.pageCount)
    || (value.pageCount as number) < 1
    || typeof value.createdAt !== 'string'
    || !isCanonicalIsoDate(value.createdAt)
    || value.sourceFileName !== 'source.pdf') {
    throw new Error(`Imported template ${index + 1} metadata is invalid.`);
  }
  return value as unknown as StoredTemplate;
}

async function assertPrivateDirectory(path: string, label: string): Promise<void> {
  const metadata = await lstat(path);
  if (!metadata.isDirectory() || metadata.isSymbolicLink() || !isCurrentUserOwned(metadata.uid)) {
    throw new Error(`The ${label} is not a private owned directory.`);
  }
}

async function readPrivateRegularFile(path: string, maximumBytes: number): Promise<Uint8Array> {
  let handle;
  try {
    handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  } catch {
    throw new Error(`The migration source ${basename(path)} cannot be opened safely.`);
  }
  try {
    const before = await handle.stat();
    if (!before.isFile()
      || before.nlink !== 1
      || !isCurrentUserOwned(before.uid)
      || before.size < 1
      || before.size > maximumBytes) {
      throw new Error(`The migration source ${basename(path)} is not a bounded private regular file.`);
    }
    const bytes = await handle.readFile();
    const after = await handle.stat();
    if (before.dev !== after.dev
      || before.ino !== after.ino
      || before.size !== after.size
      || before.mtimeMs !== after.mtimeMs
      || bytes.byteLength !== before.size) {
      throw new Error(`The migration source ${basename(path)} changed while it was read.`);
    }
    return bytes;
  } finally {
    await handle.close();
  }
}

function isCurrentUserOwned(uid: number): boolean {
  return typeof process.getuid !== 'function' || uid === process.getuid();
}

function isCanonicalIsoDate(value: string): boolean {
  const timestamp = Date.parse(value);
  return Number.isFinite(timestamp) && new Date(timestamp).toISOString() === value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  return sameStrings(Object.keys(value).sort(), [...keys].sort());
}

function sameStrings(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function isFileSystemError(error: unknown, code: string): error is NodeJS.ErrnoException {
  return error instanceof Error && 'code' in error && error.code === code;
}
