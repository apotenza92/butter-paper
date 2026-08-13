import { rmSync } from 'node:fs';
import { mkdtemp, mkdir, readdir, realpath, rm, writeFile } from 'node:fs/promises';
import { basename, dirname, join, relative, resolve, sep } from 'node:path';
import { createBlankPdf } from '@butter-paper/pdf';
import type { BlankPdfCreateRequest, BlankPdfCreateResult } from '../shared/protocol';

export class BlankPdfTemporaryStore {
  private sessionDirectoryPromise: Promise<string> | null = null;
  private sessionDirectory: string | null = null;
  private nextDocumentNumber = 1;

  constructor(
    private readonly temporaryRoot: string,
    private readonly sessionPrefix = 'butter-paper-blank-',
  ) {}

  async cleanupStaleSessions(): Promise<void> {
    const entries = await readdir(this.temporaryRoot, { withFileTypes: true }).catch((error: unknown) => {
      if (isFileSystemError(error, 'ENOENT')) return [];
      throw error;
    });
    await Promise.all(entries
      .filter((entry) => entry.isDirectory() && entry.name.startsWith(this.sessionPrefix))
      .map((entry) => rm(join(this.temporaryRoot, entry.name), { recursive: true, force: true })));
  }

  async create(request: BlankPdfCreateRequest): Promise<BlankPdfCreateResult> {
    const bytes = await createBlankPdf(request);
    return this.createFromBytes(bytes);
  }

  async createFromBytes(bytes: Uint8Array): Promise<BlankPdfCreateResult> {
    const sessionDirectory = await this.getSessionDirectory();
    const documentNumber = this.nextDocumentNumber++;
    const fileName = documentNumber === 1 ? 'Untitled.pdf' : `Untitled ${documentNumber}.pdf`;
    const documentDirectory = join(sessionDirectory, `document-${documentNumber}`);
    const temporarySourcePath = join(documentDirectory, fileName);

    await mkdir(documentDirectory, { recursive: true });
    await writeFile(temporarySourcePath, bytes);

    return {
      filePath: temporarySourcePath,
      fileName,
      temporarySourcePath,
    };
  }

  async release(temporarySourcePath: string): Promise<void> {
    const sessionDirectory = await this.sessionDirectoryPromise;
    if (!sessionDirectory || !isPathInside(sessionDirectory, temporarySourcePath)) {
      throw new Error('Temporary PDF path is outside the active Butter Paper session.');
    }
    await rm(dirname(temporarySourcePath), { recursive: true, force: true });
  }

  async cleanup(): Promise<void> {
    const sessionDirectory = this.sessionDirectory ?? await this.sessionDirectoryPromise;
    this.sessionDirectoryPromise = null;
    this.sessionDirectory = null;
    if (sessionDirectory) {
      await rm(sessionDirectory, { recursive: true, force: true });
    }
  }

  cleanupSync(): void {
    if (this.sessionDirectory) {
      rmSync(this.sessionDirectory, { recursive: true, force: true });
    }
    this.sessionDirectoryPromise = null;
    this.sessionDirectory = null;
  }

  private getSessionDirectory(): Promise<string> {
    this.sessionDirectoryPromise ??= mkdtemp(join(this.temporaryRoot, this.sessionPrefix))
      .then((sessionDirectory) => realpath(sessionDirectory))
      .then((sessionDirectory) => {
        this.sessionDirectory = sessionDirectory;
        return sessionDirectory;
      });
    return this.sessionDirectoryPromise;
  }
}

function isFileSystemError(error: unknown, code: string): error is NodeJS.ErrnoException {
  return error instanceof Error && 'code' in error && error.code === code;
}

function isPathInside(parentPath: string, childPath: string): boolean {
  const resolvedParent = resolve(parentPath);
  const resolvedChild = resolve(childPath);
  const childRelativePath = relative(resolvedParent, resolvedChild);
  return childRelativePath.length > 0
    && childRelativePath !== '..'
    && !childRelativePath.startsWith(`..${sep}`)
    && basename(resolvedChild).toLowerCase().endsWith('.pdf');
}
