import { createHash } from 'node:crypto';
import { constants } from 'node:fs';
import { lstat, mkdir, open, readdir, rename, rm } from 'node:fs/promises';
import { basename, dirname, isAbsolute, join } from 'node:path';
import type {
  ApplicationMetadata,
  ElectronMigrationExportRequest,
  ElectronMigrationExportResult,
  ImportedPdfTemplateRecord,
  UpdateFrequency,
} from '../shared/protocol';
import type { PdfTemplateMigrationSnapshot } from './pdfTemplateStore';

const MIGRATION_SCHEMA = 'butter-paper/electron-native-migration';
const MIGRATION_VERSION = 1;
const BUILT_IN_TEMPLATE_IDS = new Set([
  'built-in-blank', 'built-in-dots', 'built-in-grid',
  'built-in-lined', 'built-in-isometric', 'built-in-triangle',
]);
const MAX_MIGRATION_TEMPLATES = 256;
const MAX_MANIFEST_BYTES = 1024 * 1024;
const MAX_SOURCE_BYTES = 256 * 1024 * 1024;
const EXPORT_PARENT = 'gpui-migration-export';
const EXPORT_DIRECTORY = 'v1';
const PREVIOUS_DIRECTORY = '.v1.previous';
const UPDATE_FREQUENCIES = new Set<UpdateFrequency>([
  'never', 'startup', 'hourly', 'sixHours', 'twelveHours', 'daily', 'weekly', 'monthly',
]);

export interface ElectronMigrationExportPlan {
  readonly exportId: string;
  readonly createdAt: string;
  readonly manifestBytes: Uint8Array;
  readonly files: readonly { readonly path: string; readonly bytes: Uint8Array }[];
}

export type ElectronMigrationPublication = ElectronMigrationExportResult & {
  readonly outcome: 'created' | 'already-present';
};

export async function installDownloadedAfterMigration(
  exportMigration: () => Promise<unknown>,
  installDownloaded: (publication: ElectronMigrationPublication) => Promise<boolean>,
): Promise<void> {
  const publication = await exportMigration();
  if (!isMigrationPublication(publication)) {
    throw new Error('Migration export did not return valid publication evidence.');
  }
  if (!await installDownloaded(publication)) {
    throw new Error('No downloaded Butter Paper update is ready to install.');
  }
}

export function isMigrationPublication(value: unknown): value is ElectronMigrationPublication {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const publication = value as Record<string, unknown>;
  return (publication.outcome === 'created' || publication.outcome === 'already-present')
    && typeof publication.exportId === 'string'
    && /^[0-9a-f]{64}$/.test(publication.exportId)
    && typeof publication.createdAt === 'string'
    && isCanonicalIsoDate(publication.createdAt);
}

/**
 * Confirms that a receipt describes the currently published, fully validated
 * migration tree below the trusted Electron user-data root. Structural receipt
 * validation alone is insufficient because an in-process caller could forge it.
 */
export async function verifyElectronMigrationPublication(
  userDataRoot: string,
  value: unknown,
): Promise<boolean> {
  if (!isAbsolute(userDataRoot) || !isMigrationPublication(value)) return false;
  const published = await readValidExportIfPresent(join(userDataRoot, EXPORT_PARENT, EXPORT_DIRECTORY));
  return published?.exportId === value.exportId && published.createdAt === value.createdAt;
}

/**
 * Builds the complete, narrow migration payload from strict snapshots. This is
 * intentionally pure: publication and crash recovery are a separate boundary.
 */
export function createElectronMigrationExportPlan(options: {
  readonly metadata: ApplicationMetadata;
  readonly renderer: ElectronMigrationExportRequest;
  readonly imported: PdfTemplateMigrationSnapshot;
  readonly updateSettings: {
    readonly frequency: UpdateFrequency;
    readonly lastSuccessfulCheckAt: string | null;
  };
  readonly createdAt: string;
}): ElectronMigrationExportPlan {
  const { metadata, renderer, imported, updateSettings, createdAt } = options;
  if (metadata.development) throw new Error('Development builds cannot publish production migration data.');
  const expectedProductName = metadata.channel === 'beta' ? 'Butter Paper Beta' : 'Butter Paper';
  if (metadata.productName !== expectedProductName) throw new Error('Packaged migration identity is inconsistent.');
  const createdTimestamp = Date.parse(createdAt);
  if (!Number.isFinite(createdTimestamp) || new Date(createdTimestamp).toISOString() !== createdAt) {
    throw new Error('Migration creation time is invalid.');
  }
  if (!UPDATE_FREQUENCIES.has(updateSettings.frequency)
    || (updateSettings.lastSuccessfulCheckAt !== null
      && !isCanonicalIsoDate(updateSettings.lastSuccessfulCheckAt))) {
    throw new Error('Updater migration state is invalid.');
  }
  const bundleIdentifier = metadata.channel === 'beta'
    ? 'com.butterpaper.desktop.beta'
    : 'com.butterpaper.desktop';
  const sourcesById = new Map(imported.sources.map((source) => [source.id, source.bytes]));
  if (sourcesById.size !== imported.sources.length || imported.records.length !== imported.sources.length) {
    throw new Error('Imported template migration sources are incomplete or duplicated.');
  }
  const recordIds = new Set(imported.records.map(({ id }) => id));
  validateRendererSnapshot(renderer, recordIds);
  const importedTemplates = imported.records.map((record) => exportedImportedTemplate(record, sourcesById));
  if (recordIds.size !== imported.records.length
    || [...sourcesById.keys()].some((id) => !recordIds.has(id))) {
    throw new Error('Imported template migration sources do not match the index.');
  }
  const generated = renderer.generatedTemplates.map((template) => ({
    id: template.id,
    name: template.name,
    title: template.title,
    widthMm: template.request.widthMm,
    heightMm: template.request.heightMm,
    pattern: template.request.pattern
      ? {
          kind: template.request.pattern.type,
          spacingMm: template.request.pattern.spacingMm,
          color: template.request.pattern.color,
        }
      : null,
  }));
  const exportIdentity = {
    channel: metadata.channel,
    source: {
      productName: metadata.productName,
      bundleIdentifier,
      version: metadata.version,
    },
    preferences: {
      menuBarVisible: renderer.menuBarVisible,
      updateFrequency: updateSettings.frequency,
      lastSuccessfulUpdateCheckAt: updateSettings.lastSuccessfulCheckAt,
    },
    templates: {
      lastTemplateId: renderer.lastTemplateId,
      generated,
      imported: importedTemplates,
    },
    unsupported: {
      recentSignatures: 'requires-secure-bridge',
      session: 'not-persisted-by-electron',
      colourPresets: 'not-imported-v1',
    },
  } as const;
  const exportId = sha256(Buffer.from(canonicalJson(exportIdentity)));
  const manifest = {
    schema: MIGRATION_SCHEMA,
    version: MIGRATION_VERSION,
    channel: metadata.channel,
    exportId,
    createdAt,
    source: exportIdentity.source,
    preferences: exportIdentity.preferences,
    templates: exportIdentity.templates,
    unsupported: exportIdentity.unsupported,
  };
  return {
    exportId,
    createdAt,
    manifestBytes: Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`),
    files: imported.records.map((record) => ({
      path: migrationSourcePath(record.id),
      bytes: sourcesById.get(record.id)!,
    })),
  };
}

/** Publishes a plan below an explicitly supplied disposable or Electron user-data root. */
export async function publishElectronMigrationExport(
  userDataRoot: string,
  plan: ElectronMigrationExportPlan,
): Promise<ElectronMigrationPublication> {
  if (!isAbsolute(userDataRoot)) throw new Error('Migration user-data root must be absolute.');
  const parent = join(userDataRoot, EXPORT_PARENT);
  const current = join(parent, EXPORT_DIRECTORY);
  const previous = join(parent, PREVIOUS_DIRECTORY);
  const stageName = `.v1.staging-${plan.exportId}`;
  const stage = join(parent, stageName);
  const owner = join(parent, `${stageName}.owner`);
  await mkdir(parent, { recursive: true, mode: 0o700 });
  await assertOwnedDirectory(parent, 'migration exchange parent');
  await recoverOwnedStages(parent);

  const currentState = await readValidExportIfPresent(current);
  const previousState = await readValidExportIfPresent(previous);
  if (!currentState && previousState) {
    if (await pathExists(current)) throw new Error('Migration exchange has an invalid current tree.');
    await rename(previous, current);
    await syncDirectory(parent);
  } else if (currentState && previousState) {
    await rm(previous, { recursive: true });
    await syncDirectory(parent);
  } else if (!currentState && await pathExists(current)) {
    throw new Error('Migration exchange has an invalid current tree.');
  } else if (!previousState && await pathExists(previous)) {
    throw new Error('Migration exchange has an invalid recovery tree.');
  }

  const recoveredCurrent = await readValidExportIfPresent(current);
  if (recoveredCurrent?.exportId === plan.exportId) {
    return { outcome: 'already-present', exportId: recoveredCurrent.exportId, createdAt: recoveredCurrent.createdAt };
  }
  await writePrivateFile(owner, Buffer.from(`${plan.exportId}\n`));
  try {
    await createPrivateDirectory(stage);
    for (const file of plan.files) {
      const destination = join(stage, ...file.path.split('/'));
      await mkdir(dirname(destination), { recursive: true, mode: 0o700 });
      await writePrivateFile(destination, file.bytes);
    }
    await writePrivateFile(join(stage, 'manifest.json'), plan.manifestBytes);
    await syncTreeDirectories(stage);
    const staged = await readValidExportIfPresent(stage);
    if (!staged || staged.exportId !== plan.exportId) throw new Error('Staged migration export failed verification.');

    if (recoveredCurrent) {
      await rename(current, previous);
      await syncDirectory(parent);
    }
    await rename(stage, current);
    await syncDirectory(parent);
    const published = await readValidExportIfPresent(current);
    if (!published || published.exportId !== plan.exportId) throw new Error('Published migration export failed verification.');
    if (await pathExists(previous)) await rm(previous, { recursive: true });
    await rm(owner);
    await syncDirectory(parent);
    return { outcome: 'created', exportId: published.exportId, createdAt: published.createdAt };
  } catch (error) {
    // Leave exact owned state for deterministic recovery on the next attempt.
    throw error;
  }
}

function validateRendererSnapshot(
  renderer: ElectronMigrationExportRequest,
  importedIds: ReadonlySet<string>,
): void {
  if (typeof renderer.menuBarVisible !== 'boolean'
    || typeof renderer.lastTemplateId !== 'string'
    || !Array.isArray(renderer.generatedTemplates)
    || renderer.generatedTemplates.length + importedIds.size > MAX_MIGRATION_TEMPLATES) {
    throw new Error('Renderer migration state is invalid.');
  }
  const ids = new Set(importedIds);
  for (const template of renderer.generatedTemplates) {
    if (!template || typeof template !== 'object' || Array.isArray(template)) {
      throw new Error('Renderer migration state is invalid.');
    }
    const requestKeys = template.request && typeof template.request === 'object'
      ? Object.keys(template.request).sort().join(',')
      : '';
    if (Object.keys(template).sort().join(',') !== 'id,name,request,title'
      || (requestKeys !== 'heightMm,widthMm' && requestKeys !== 'heightMm,pattern,widthMm')
      || typeof template.id !== 'string'
      || !/^custom-[a-z0-9_-]+$/.test(template.id)
      || template.id.length > 128
      || ids.has(template.id)
      || typeof template.name !== 'string'
      || template.name.length === 0
      || template.name.length > 80
      || template.name !== template.name.trim()
      || [...template.name].some((character) => /\p{Cc}/u.test(character))
      || template.title !== 'Untitled'
      || !validDimension(template.request?.widthMm)
      || !validDimension(template.request?.heightMm)
      || !validPattern(template.request?.pattern)) {
      throw new Error('Renderer migration state is invalid.');
    }
    ids.add(template.id);
  }
  if (!BUILT_IN_TEMPLATE_IDS.has(renderer.lastTemplateId) && !ids.has(renderer.lastTemplateId)) {
    throw new Error('Renderer migration last selection is unavailable.');
  }
}

function validDimension(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value) && value >= 10 && value <= 5_000;
}

function validPattern(value: unknown): boolean {
  if (value === undefined) return true;
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const pattern = value as Record<string, unknown>;
  return Object.keys(pattern).sort().join(',') === 'color,spacingMm,type'
    && ['dots', 'grid', 'lined', 'isometric', 'triangle'].includes(pattern.type as string)
    && typeof pattern.spacingMm === 'number'
    && Number.isFinite(pattern.spacingMm)
    && pattern.spacingMm >= 1
    && pattern.spacingMm <= 500
    && typeof pattern.color === 'string'
    && /^#[0-9a-f]{6}$/.test(pattern.color);
}

function exportedImportedTemplate(
  record: ImportedPdfTemplateRecord,
  sourcesById: Map<string, Uint8Array>,
) {
  const bytes = sourcesById.get(record.id);
  if (!bytes) throw new Error(`Imported template ${record.id} has no migration source.`);
  return {
    id: record.id,
    name: record.name,
    createdAt: record.createdAt,
    pageCount: record.pageCount,
    source: {
      path: migrationSourcePath(record.id),
      bytes: bytes.byteLength,
      sha256: sha256(bytes),
    },
  };
}

function migrationSourcePath(id: string): string {
  return `templates/${id}/source.pdf`;
}

function canonicalJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (value !== null && typeof value === 'object') {
    return `{${Object.entries(value as Record<string, unknown>)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, child]) => `${JSON.stringify(key)}:${canonicalJson(child)}`)
      .join(',')}}`;
  }
  return JSON.stringify(value);
}

interface ValidPublishedExport {
  readonly exportId: string;
  readonly createdAt: string;
}

async function readValidExportIfPresent(root: string): Promise<ValidPublishedExport | null> {
  let metadata;
  try {
    metadata = await lstat(root);
  } catch (error) {
    if (isFileSystemError(error, 'ENOENT')) return null;
    throw error;
  }
  if (!metadata.isDirectory() || metadata.isSymbolicLink() || !isCurrentUserOwned(metadata.uid)) {
    throw new Error(`Migration export ${basename(root)} is not an owned directory.`);
  }
  const manifestBytes = await readPrivateRegularFile(join(root, 'manifest.json'), MAX_MANIFEST_BYTES);
  let manifest: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(Buffer.from(manifestBytes).toString('utf8'));
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error();
    manifest = parsed as Record<string, unknown>;
  } catch {
    throw new Error(`Migration export ${basename(root)} has an invalid manifest.`);
  }
  if (!sameStrings(Object.keys(manifest).sort(), [
    'channel', 'createdAt', 'exportId', 'preferences', 'schema', 'source', 'templates', 'unsupported', 'version',
  ])
    || manifest.schema !== MIGRATION_SCHEMA
    || manifest.version !== MIGRATION_VERSION
    || typeof manifest.exportId !== 'string'
    || !/^[0-9a-f]{64}$/.test(manifest.exportId)
    || typeof manifest.createdAt !== 'string'
    || !isCanonicalIsoDate(manifest.createdAt)) {
    throw new Error(`Migration export ${basename(root)} has an invalid manifest identity.`);
  }
  const templates = manifest.templates;
  if (!templates || typeof templates !== 'object' || Array.isArray(templates)) {
    throw new Error(`Migration export ${basename(root)} has invalid templates.`);
  }
  const imported = (templates as Record<string, unknown>).imported;
  if (!Array.isArray(imported) || imported.length > MAX_MIGRATION_TEMPLATES) {
    throw new Error(`Migration export ${basename(root)} has invalid imported templates.`);
  }
  const identifiers = new Set<string>();
  let totalBytes = 0;
  if (imported.length > 0) await assertOwnedDirectory(join(root, 'templates'), 'migration templates directory');
  for (const value of imported) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
      throw new Error(`Migration export ${basename(root)} has invalid imported templates.`);
    }
    const template = value as Record<string, unknown>;
    const source = template.source;
    if (typeof template.id !== 'string'
      || !/^imported-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(template.id)
      || identifiers.has(template.id)
      || !source
      || typeof source !== 'object'
      || Array.isArray(source)) {
      throw new Error(`Migration export ${basename(root)} has invalid imported templates.`);
    }
    const sourceRecord = source as Record<string, unknown>;
    const expectedPath = migrationSourcePath(template.id);
    if (sourceRecord.path !== expectedPath
      || !Number.isSafeInteger(sourceRecord.bytes)
      || (sourceRecord.bytes as number) < 1
      || (sourceRecord.bytes as number) > MAX_SOURCE_BYTES
      || typeof sourceRecord.sha256 !== 'string'
      || !/^[0-9a-f]{64}$/.test(sourceRecord.sha256)) {
      throw new Error(`Migration export ${basename(root)} has invalid imported sources.`);
    }
    await assertOwnedDirectory(join(root, 'templates', template.id), 'migration template directory');
    const sourceBytes = await readPrivateRegularFile(join(root, ...expectedPath.split('/')), MAX_SOURCE_BYTES);
    totalBytes += sourceBytes.byteLength;
    if (sourceBytes.byteLength !== sourceRecord.bytes
      || sha256(sourceBytes) !== sourceRecord.sha256
      || totalBytes > 1024 * 1024 * 1024) {
      throw new Error(`Migration export ${basename(root)} source verification failed.`);
    }
    identifiers.add(template.id);
  }
  const identity = {
    channel: manifest.channel,
    source: manifest.source,
    preferences: manifest.preferences,
    templates: manifest.templates,
    unsupported: manifest.unsupported,
  };
  if (sha256(Buffer.from(canonicalJson(identity))) !== manifest.exportId) {
    throw new Error(`Migration export ${basename(root)} identity checksum failed.`);
  }
  const rootEntries = (await readdir(root)).sort();
  const expectedRoot = imported.length === 0 ? ['manifest.json'] : ['manifest.json', 'templates'];
  if (!sameStrings(rootEntries, expectedRoot)) throw new Error(`Migration export ${basename(root)} has extra entries.`);
  if (imported.length > 0) {
    await assertOwnedDirectory(join(root, 'templates'), 'migration templates directory');
    if (!sameStrings((await readdir(join(root, 'templates'))).sort(), [...identifiers].sort())) {
      throw new Error(`Migration export ${basename(root)} has an invalid template inventory.`);
    }
    for (const id of identifiers) {
      const directory = join(root, 'templates', id);
      await assertOwnedDirectory(directory, 'migration template directory');
      if (!sameStrings(await readdir(directory), ['source.pdf'])) {
        throw new Error(`Migration export ${basename(root)} has an invalid source inventory.`);
      }
    }
  }
  return { exportId: manifest.exportId, createdAt: manifest.createdAt };
}

async function recoverOwnedStages(parent: string): Promise<void> {
  const entries = new Set(await readdir(parent));
  for (const entry of [...entries]) {
    if (!entries.has(entry)) continue;
    if (entry === EXPORT_DIRECTORY || entry === PREVIOUS_DIRECTORY) continue;
    const match = entry.match(/^\.v1\.staging-([0-9a-f]{64})(\.owner)?$/);
    if (!match) throw new Error('Migration exchange parent contains an unrecognised entry.');
    const stageName = `.v1.staging-${match[1]}`;
    const ownerName = `${stageName}.owner`;
    if (!entries.has(ownerName)) throw new Error('Migration exchange contains an unowned staging tree.');
    const ownerBytes = await readPrivateRegularFile(join(parent, ownerName), 256);
    if (Buffer.from(ownerBytes).toString('utf8') !== `${match[1]}\n`) {
      throw new Error('Migration exchange staging ownership is invalid.');
    }
    if (entries.has(stageName)) {
      await assertOwnedDirectory(join(parent, stageName), 'migration staging tree');
      await rm(join(parent, stageName), { recursive: true });
      entries.delete(stageName);
    }
    await rm(join(parent, ownerName));
    entries.delete(ownerName);
  }
  await syncDirectory(parent);
}

async function createPrivateDirectory(path: string): Promise<void> {
  await mkdir(path, { mode: 0o700 });
  await assertOwnedDirectory(path, 'migration staging directory');
}

async function assertOwnedDirectory(path: string, label: string): Promise<void> {
  const metadata = await lstat(path);
  if (!metadata.isDirectory() || metadata.isSymbolicLink() || !isCurrentUserOwned(metadata.uid)) {
    throw new Error(`The ${label} is not a private owned directory.`);
  }
}

async function writePrivateFile(path: string, bytes: Uint8Array): Promise<void> {
  const handle = await open(path, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
  try {
    await handle.writeFile(bytes);
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function readPrivateRegularFile(path: string, maximumBytes: number): Promise<Uint8Array> {
  const handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    const before = await handle.stat();
    if (!before.isFile()
      || before.nlink !== 1
      || !isCurrentUserOwned(before.uid)
      || before.size < 1
      || before.size > maximumBytes) {
      throw new Error(`Migration file ${basename(path)} is not a bounded private regular file.`);
    }
    const bytes = await handle.readFile();
    const after = await handle.stat();
    if (before.dev !== after.dev
      || before.ino !== after.ino
      || before.size !== after.size
      || before.mtimeMs !== after.mtimeMs
      || bytes.byteLength !== before.size) {
      throw new Error(`Migration file ${basename(path)} changed while it was read.`);
    }
    return bytes;
  } finally {
    await handle.close();
  }
}

async function syncTreeDirectories(root: string): Promise<void> {
  const templates = join(root, 'templates');
  if (await pathExists(templates)) {
    for (const id of await readdir(templates)) await syncDirectory(join(templates, id));
    await syncDirectory(templates);
  }
  await syncDirectory(root);
}

async function syncDirectory(path: string): Promise<void> {
  const handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  try {
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function pathExists(path: string): Promise<boolean> {
  try {
    await lstat(path);
    return true;
  } catch (error) {
    if (isFileSystemError(error, 'ENOENT')) return false;
    throw error;
  }
}

function isCanonicalIsoDate(value: string): boolean {
  const timestamp = Date.parse(value);
  return Number.isFinite(timestamp) && new Date(timestamp).toISOString() === value;
}

function isCurrentUserOwned(uid: number): boolean {
  return typeof process.getuid !== 'function' || uid === process.getuid();
}

function sameStrings(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function isFileSystemError(error: unknown, code: string): error is NodeJS.ErrnoException {
  return error instanceof Error && 'code' in error && error.code === code;
}

function sha256(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}
