export async function saveDocumentsInOrder<T>(
  documents: readonly T[],
  saveDocument: (document: T) => Promise<boolean>,
): Promise<boolean> {
  for (const document of documents) {
    if (!await saveDocument(document)) {
      return false;
    }
  }
  return true;
}
