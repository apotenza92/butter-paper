export function selectDroppedPdfFiles(files: Iterable<File>): File[] {
  return Array.from(files).filter((file) => /\.pdf$/i.test(file.name));
}
