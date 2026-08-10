import type { PdfValidationEvidenceInventory } from '@butter-paper/core';

const absentCollection = {
  present: false,
  referenceCount: 0,
  embeddedObjectCount: 0,
  malformedEntryCount: 0,
  inspectionComplete: true,
} as const;

/** Explicit fixture data. Production report guards never synthesize this for missing fields. */
export const absentValidationEvidence: PdfValidationEvidenceInventory = {
  dssPresent: false,
  vriPresent: false,
  structureStatus: 'absent',
  inventoryComplete: true,
  limitExceeded: false,
  certificates: absentCollection,
  ocspResponses: absentCollection,
  crls: absentCollection,
  vriEntryCount: 0,
  vriEntries: [],
};
