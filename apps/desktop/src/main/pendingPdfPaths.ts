const pendingPdfPaths: string[] = [];

export function enqueuePendingPdfPaths(filePaths: readonly string[]): void {
  for (const filePath of filePaths) {
    if (!pendingPdfPaths.includes(filePath)) {
      pendingPdfPaths.push(filePath);
    }
  }
}

export function takePendingPdfPaths(): string[] {
  return pendingPdfPaths.splice(0);
}

export function hasPendingPdfPaths(): boolean {
  return pendingPdfPaths.length > 0;
}
