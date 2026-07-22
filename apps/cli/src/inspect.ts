import { openPdfDocument } from '@butter-paper/pdf';

export interface PageInspection {
  pageNumber: number;
  width: number;
  height: number;
  rotation: number;
}

export interface PdfInspection {
  path: string;
  pageCount: number;
  metadata: {
    title?: string;
    author?: string;
    subject?: string;
    creator?: string;
    producer?: string;
  };
  pages: PageInspection[];
}

export async function inspectPdf(pdfPath: string): Promise<PdfInspection> {
  const document = await openPdfDocument(pdfPath);

  try {
    const metadata = await document.getMetadata();
    const pageCount = document.pageCount;
    const pages: PageInspection[] = [];

    for (let pageIndex = 0; pageIndex < pageCount; pageIndex += 1) {
      const page = await document.getPageInfo(pageIndex);
      pages.push({
        pageNumber: pageIndex + 1,
        width: page.width,
        height: page.height,
        rotation: page.rotation,
      });
    }

    return {
      path: pdfPath,
      pageCount,
      metadata: {
        title: metadata.title ?? undefined,
        author: metadata.author ?? undefined,
        subject: metadata.subject ?? undefined,
        creator: metadata.creator ?? undefined,
        producer: metadata.producer ?? undefined,
      },
      pages,
    };
  } finally {
    await document.close();
  }
}
